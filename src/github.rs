use failure::Error;
use reqwest::{self, blocking::Client, blocking::Response};
use serde::{Deserialize, Serialize};

use crate::Commit;

#[derive(Serialize, Deserialize, Debug)]
struct GithubCommitElem {
    commit: GithubCommit,
    sha: String,
}
#[derive(Serialize, Deserialize, Debug)]
struct GithubCommit {
    author: GithubAuthor,
    committer: GithubAuthor,
    message: String,
}
#[derive(Serialize, Deserialize, Debug)]
struct GithubAuthor {
    date: String,
    email: String,
    name: String,
}

type GitDate = chrono::DateTime<chrono::Utc>;

impl GithubCommitElem {
    fn date(&self) -> Result<GitDate, Error> {
        Ok(self.commit.committer.date.parse()?)
    }

    fn git_commit(self) -> Result<Commit, Error> {
        let date = self.date()?;
        Ok(Commit {
            sha: self.sha,
            date,
            summary: self.commit.message,
        })
    }
}

fn headers() -> Result<reqwest::header::HeaderMap, Error> {
    let mut headers = reqwest::header::HeaderMap::new();
    let user_agent = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
    let user_agent = reqwest::header::HeaderValue::from_static(user_agent);
    headers.insert(reqwest::header::USER_AGENT, user_agent);
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        eprintln!("adding local env GITHUB_TOKEN value to headers in github query");
        let value = reqwest::header::HeaderValue::from_str(&format!("token {}", token))?;
        headers.insert(reqwest::header::AUTHORIZATION, value);
    }
    Ok(headers)
}

pub(crate) fn get_commit(sha: &str) -> Result<Commit, Error> {
    let url = SingleCommitUrl { sha }.url();
    let client = Client::builder().default_headers(headers()?).build()?;
    let response: Response = client.get(&url).send()?;
    let elem: GithubCommitElem = response.json()?;
    elem.git_commit()
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct CommitsQuery<'a> {
    pub since_date: &'a str,
    pub most_recent_sha: &'a str,
    pub earliest_sha: &'a str,
}

/// Returns the bors merge commits between the two specified boundaries
/// (boundaries inclusive).

impl<'a> CommitsQuery<'a> {
    pub fn get_commits(&self) -> Result<Vec<Commit>, Error> {
        get_commits(*self)
    }
}

const PER_PAGE: usize = 100;
const OWNER: &str = "rust-lang";
const REPO: &str = "rust";

trait ToUrl {
    fn url(&self) -> String;
}
struct CommitsUrl<'a> {
    page: usize,
    author: &'a str,
    since: &'a str,
    sha: &'a str,
}
struct SingleCommitUrl<'a> {
    sha: &'a str,
}

impl<'a> ToUrl for CommitsUrl<'a> {
    fn url(&self) -> String {
        format!(
            "https://api.github.com/repos/{OWNER}/{REPO}/commits\
                 ?page={PAGE}&per_page={PER_PAGE}\
                 &author={AUTHOR}&since={SINCE}&sha={SHA}",
            OWNER = OWNER,
            REPO = REPO,
            PAGE = self.page,
            PER_PAGE = PER_PAGE,
            AUTHOR = self.author,
            SINCE = self.since,
            SHA = self.sha
        )
    }
}

impl<'a> ToUrl for SingleCommitUrl<'a> {
    fn url(&self) -> String {
        // "origin/master" is set as `sha` when there is no `--end=` definition
        // specified on the command line.  We define the GitHub master branch
        // HEAD commit as the end commit in this case
        if self.sha == "origin/master" {
            format!(
                "https://api.github.com/repos/{OWNER}/{REPO}/commits/master",
                OWNER = OWNER,
                REPO = REPO,
            )
        } else {
            format!(
                "https://api.github.com/repos/{OWNER}/{REPO}/commits/{REF}",
                OWNER = OWNER,
                REPO = REPO,
                REF = self.sha
            )
        }
    }
}

fn get_commits(q: CommitsQuery) -> Result<Vec<Commit>, Error> {
    // build up commit sequence, by feeding in `sha` as the starting point, and
    // working way backwards to max(`q.since_date`, `q.earliest_sha`).
    let mut commits = Vec::new();

    // focus on Pull Request merges, all authored and committed by bors.
    let author = "bors";

    let client = Client::builder().default_headers(headers()?).build()?;
    for page in 1.. {
        let url = CommitsUrl {
            page,
            author,
            since: q.since_date,
            sha: q.most_recent_sha,
        }
        .url();

        let response: Response = client.get(&url).send()?;

        let action = parse_paged_elems(response, |elem: GithubCommitElem| {
            let date: chrono::DateTime<chrono::Utc> = match elem.commit.committer.date.parse() {
                Ok(date) => date,
                Err(err) => return Loop::Err(err.into()),
            };
            let sha = elem.sha.clone();
            let summary = elem.commit.message;
            let commit = Commit { sha, date, summary };
            commits.push(commit);

            if elem.sha == q.earliest_sha {
                eprintln!(
                    "ending github query because we found starting sha: {}",
                    elem.sha
                );
                return Loop::Break;
            }

            Loop::Next
        })?;

        if let Loop::Break = action {
            break;
        }
    }

    eprintln!(
        "get_commits_between returning commits, len: {}",
        commits.len()
    );

    // reverse to obtain chronological order
    commits.reverse();
    Ok(commits)
}

enum Loop<E> {
    Break,
    Next,
    Err(E),
}
enum Void {}

fn parse_paged_elems<Elem: for<'a> serde::Deserialize<'a>>(
    response: Response,
    mut k: impl FnMut(Elem) -> Loop<Error>,
) -> Result<Loop<Void>, Error> {
    // parse the JSON into an array of the expected Elem type
    let elems: Vec<Elem> = response.json()?;

    if elems.is_empty() {
        // we've run out of useful pages to lookup
        return Ok(Loop::Break);
    }

    for elem in elems.into_iter() {
        let act = k(elem);

        // the callback will tell us if we should terminate loop early (e.g. due to matching `sha`)
        match act {
            Loop::Break => return Ok(Loop::Break),
            Loop::Err(e) => return Err(e),
            Loop::Next => continue,
        }
    }

    // by default, we keep searching on next page from github.
    Ok(Loop::Next)
}

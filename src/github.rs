use anyhow::{bail, Context};
use reqwest::header::{HeaderMap, HeaderValue, InvalidHeaderValue, AUTHORIZATION, USER_AGENT};
use reqwest::{blocking::Client, blocking::Response};
use serde::{Deserialize, Serialize};

use crate::{parse_to_naive_date, Author, Commit, GitDate, BORS_AUTHOR};

#[derive(Serialize, Deserialize, Debug)]
struct GithubCommitComparison {
    merge_base_commit: GithubCommitElem,
}
#[derive(Serialize, Deserialize, Debug)]
struct GithubCommitElem {
    commit: GithubCommit,
    sha: String,
}
#[derive(Serialize, Deserialize, Debug)]
struct GithubCommit {
    author: Option<GithubAuthor>,
    committer: Option<GithubAuthor>,
    message: String,
}
#[derive(Serialize, Deserialize, Debug)]
struct GithubAuthor {
    date: String,
    email: String,
    name: String,
}
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct GithubCommentAuthor {
    pub(crate) login: String,
}
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct GithubComment {
    pub(crate) user: GithubCommentAuthor,
    pub(crate) body: String,
}

impl GithubCommitElem {
    fn date(&self) -> anyhow::Result<GitDate> {
        let (date_str, _) = self
            .commit
            .committer
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("commit should have committer"))?
            .date
            .split_once('T')
            .context("commit date should folllow the ISO 8061 format, eg: 2022-05-04T09:55:51Z")?;
        Ok(parse_to_naive_date(date_str)?)
    }

    fn git_commit(self) -> anyhow::Result<Commit> {
        let date = self.date()?;
        let committer = self
            .commit
            .committer
            .ok_or_else(|| anyhow::anyhow!("commit should have committer"))?;
        let committer = Author {
            name: committer.name,
            email: committer.email,
            date,
        };
        Ok(Commit {
            sha: self.sha,
            date,
            summary: self.commit.message,
            committer,
        })
    }
}

fn headers() -> Result<HeaderMap, InvalidHeaderValue> {
    let mut headers = HeaderMap::new();
    let user_agent = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
    let user_agent = HeaderValue::from_static(user_agent);
    headers.insert(USER_AGENT, user_agent);
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        eprintln!("adding local env GITHUB_TOKEN value to headers in github query");
        let value = HeaderValue::from_str(&format!("token {token}"))?;
        headers.insert(AUTHORIZATION, value);
    }
    Ok(headers)
}

/// Lookup information about the given commit reference from GitHub.
/// If it is a commit forked from the "mainline" git history (e.g. a stable tag
/// like `1.88.0`), returns the merge base of the given commit reference.
pub(crate) fn get_commit(sha: &str) -> anyhow::Result<Commit> {
    let url = CommitDetailsUrl { sha }.url();
    let client = Client::builder().default_headers(headers()?).build()?;
    let response: Response = client.get(&url).send()?;
    let status = response.status();
    if !status.is_success() {
        bail!(
            "error: url <{}> response {}: {}",
            url,
            status,
            response.text().unwrap_or_else(|_| format!("<empty>"))
        );
    }
    let elem: GithubCommitComparison = response
        .json()
        .with_context(|| "failed to decode GitHub JSON response")?;
    elem.merge_base_commit.git_commit()
}

pub(crate) fn get_pr_comments(pr: &str) -> anyhow::Result<Vec<GithubComment>> {
    let url = format!("https://api.github.com/repos/rust-lang/rust/issues/{pr}/comments");
    let client = Client::builder().default_headers(headers()?).build()?;
    let response: Response = client.get(&url).send()?;
    let status = response.status();
    if !status.is_success() {
        bail!(
            "error: url <{}> response {}: {}",
            url,
            status,
            response.text().unwrap_or_else(|_| format!("<empty>"))
        );
    }
    let comments: Vec<GithubComment> = response
        .json()
        .with_context(|| "failed to decode GitHub JSON response")?;
    Ok(comments)
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct CommitsQuery<'a> {
    pub since_date: &'a str,
    pub most_recent_sha: &'a str,
    pub earliest_sha: &'a str,
}

/// Returns the bors merge commits between the two specified boundaries
/// (boundaries inclusive).

impl CommitsQuery<'_> {
    pub fn get_commits(&self) -> anyhow::Result<Vec<Commit>> {
        // build up commit sequence, by feeding in `sha` as the starting point, and
        // working way backwards to max(`self.since_date`, `self.earliest_sha`).
        let mut commits = Vec::new();

        // focus on Pull Request merges, all authored and committed by bors.
        let client = Client::builder().default_headers(headers()?).build()?;
        for page in 1.. {
            let url = CommitsUrl {
                page,
                author: BORS_AUTHOR,
                since: self.since_date,
                sha: self.most_recent_sha,
            }
            .url();

            let response: Response = client.get(&url).send()?;
            let status = response.status();
            if !status.is_success() {
                bail!(
                    "error: url <{}> response {}: {}",
                    url,
                    status,
                    response.text().unwrap_or_else(|_| format!("<empty>"))
                );
            }

            let action = parse_paged_elems(response, |elem: GithubCommitElem| {
                let found_last = elem.sha == self.earliest_sha;
                if found_last {
                    eprintln!(
                        "ending github query because we found starting sha: {}",
                        elem.sha
                    );
                }
                let commit = elem.git_commit()?;
                commits.push(commit);

                Ok(if found_last { Loop::Break } else { Loop::Next })
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
struct CommitDetailsUrl<'a> {
    sha: &'a str,
}

impl ToUrl for CommitsUrl<'_> {
    fn url(&self) -> String {
        format!(
            "https://api.github.com/repos/{OWNER}/{REPO}/commits\
                 ?page={page}&per_page={PER_PAGE}\
                 &author={author}&since={since}&sha={sha}",
            page = self.page,
            author = self.author,
            since = self.since,
            sha = self.sha
        )
    }
}

impl ToUrl for CommitDetailsUrl<'_> {
    fn url(&self) -> String {
        // "origin/master" is set as `sha` when there is no `--end=` definition
        // specified on the command line.  We define the GitHub master branch
        // HEAD commit as the end commit in this case
        let reference = if self.sha == "origin/master" {
            "master"
        } else {
            self.sha
        };

        format!("https://api.github.com/repos/{OWNER}/{REPO}/compare/master...{reference}")
    }
}

enum Loop {
    Break,
    Next,
}

fn parse_paged_elems(
    response: Response,
    mut k: impl FnMut(GithubCommitElem) -> anyhow::Result<Loop>,
) -> anyhow::Result<Loop> {
    let elems: Vec<GithubCommitElem> = response.json()?;

    if elems.is_empty() {
        // we've run out of useful pages to lookup
        return Ok(Loop::Break);
    }

    for elem in elems {
        let act = k(elem)?;

        // the callback will tell us if we should terminate loop early (e.g. due to matching `sha`)
        match act {
            Loop::Break => return Ok(Loop::Break),
            Loop::Next => continue,
        }
    }

    // by default, we keep searching on next page from github.
    Ok(Loop::Next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github() {
        let c = get_commit("25674202bb7415e0c0ecd07856749cfb7f591be6").unwrap();
        let committer = Author {
            name: String::from("bors"),
            email: String::from("bors@rust-lang.org"),
            date: GitDate::from_ymd_opt(2022, 5, 4).unwrap(),
        };
        let expected_c = Commit { sha: "25674202bb7415e0c0ecd07856749cfb7f591be6".to_string(), 
                                date: parse_to_naive_date("2022-05-04").unwrap(),
                                summary: "Auto merge of #96695 - JohnTitor:rollup-oo4fc1h, r=JohnTitor\n\nRollup of 6 pull requests\n\nSuccessful merges:\n\n - #96597 (openbsd: unbreak build on native platform)\n - #96662 (Fix typo in lint levels doc)\n - #96668 (Fix flaky rustdoc-ui test because it did not replace time result)\n - #96679 (Quick fix for #96223.)\n - #96684 (Update `ProjectionElem::Downcast` documentation)\n - #96686 (Add some TAIT-related tests)\n\nFailed merges:\n\nr? `@ghost`\n`@rustbot` modify labels: rollup".to_string(),
                                committer,
                            };
        assert_eq!(c, expected_c)
    }
}

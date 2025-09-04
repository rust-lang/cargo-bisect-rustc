//! Get git commits with help of the libgit2 library

const RUST_SRC_URL: &str = "https://github.com/rust-lang/rust";
const RUST_SRC_REPO: Option<&str> = option_env!("RUST_SRC_REPO");

use std::env;
use std::ops::Deref;
use std::path::Path;

use anyhow::{bail, Context};
use chrono::{TimeZone, Utc};
use git2::build::RepoBuilder;
use git2::{Commit as Git2Commit, Repository};
use log::debug;

use crate::{Author, Commit, GitDate, BORS_AUTHOR};

impl Commit {
    // Takes &mut because libgit2 internally caches summaries
    fn from_git2_commit(commit: &mut Git2Commit<'_>) -> Self {
        let committer = commit.committer();
        Commit {
            sha: commit.id().to_string(),
            date: time_to_date(&commit.time()),
            summary: String::from_utf8_lossy(commit.summary_bytes().unwrap()).to_string(),
            committer: Author {
                name: committer.name().unwrap_or("").to_string(),
                email: committer.email().unwrap_or("").to_string(),
                date: time_to_date(&committer.when()),
            },
        }
    }
}

fn time_to_date(time: &git2::Time) -> GitDate {
    Utc.timestamp_opt(time.seconds(), 0).unwrap().date_naive()
}

struct RustcRepo {
    repository: Repository,
    origin_remote: String,
}

impl Deref for RustcRepo {
    type Target = Repository;

    fn deref(&self) -> &Self::Target {
        &self.repository
    }
}

fn lookup_rev<'rev>(repo: &'rev RustcRepo, rev: &str) -> anyhow::Result<Git2Commit<'rev>> {
    let revision = repo.revparse_single(rev)?;

    // Find the merge-base between the revision and the default branch.
    // If revision is a normal commit contained in the default branch, the merge-base will be the
    // commit itself.
    // If revision is a tag (e.g. a release version), the merge-base will contain the latest
    // default branch commit contained in that tag.
    let default_branch_id = repo
        .revparse_single(&format!("{}/HEAD", repo.origin_remote))?
        .id();
    let revision_id = revision
        .as_tag()
        .map_or_else(|| revision.id(), git2::Tag::target_id);

    let common_base = repo.merge_base(default_branch_id, revision_id)?;

    if let Ok(c) = repo.find_commit(common_base) {
        return Ok(c);
    }
    bail!("Could not find a commit for revision specifier '{}'", rev)
}

fn get_repo() -> anyhow::Result<RustcRepo> {
    fn open(path: &Path) -> anyhow::Result<(Repository, String)> {
        eprintln!("opening existing repository at {:?}", path);
        let repo = Repository::open(path)?;

        let origin_remote = find_origin_remote(&repo)?;
        eprintln!("Found origin remote under name `{origin_remote}`");

        eprintln!("refreshing repository at {:?}", path);
        // This uses the CLI because libgit2 is quite slow to fetch a large repository.
        let status = std::process::Command::new("git")
            .args(&["fetch", "--tags"])
            .arg(&origin_remote)
            .current_dir(path)
            .status()
            .context("expected `git` command-line executable to be installed".to_string())?;
        if !status.success() {
            bail!("git fetch failed exit status {}", status);
        }

        Ok((repo, origin_remote))
    }

    let loc = Path::new("rust.git");
    let (repository, origin_remote) = match (env::var_os("RUST_SRC_REPO"), RUST_SRC_REPO) {
        (Some(repo), _) => open(Path::new(&repo)),
        (None, _) if loc.exists() => open(loc),
        (None, Some(repo)) => open(Path::new(repo)),
        _ => {
            eprintln!("cloning rust repository");
            Ok((
                RepoBuilder::new().bare(true).clone(RUST_SRC_URL, loc)?,
                "origin".to_string(),
            ))
        }
    }?;

    Ok(RustcRepo {
        repository,
        origin_remote,
    })
}

fn find_origin_remote(repo: &Repository) -> anyhow::Result<String> {
    repo.remotes()?
        .iter()
        .filter_map(|name| name.and_then(|name| repo.find_remote(name).ok()))
        .find(|remote| {
            remote
                .url()
                .map_or(false, |url| url.contains("rust-lang/rust"))
        })
        .and_then(|remote| remote.name().map(std::string::ToString::to_string))
        .with_context(|| {
            format!(
                "rust-lang/rust remote not found. \
Try adding a remote pointing to `{RUST_SRC_URL}` in the rust repository at `{}`.",
                repo.path().display()
            )
        })
}

pub(crate) fn get_commit(sha: &str) -> anyhow::Result<Commit> {
    let repo = get_repo()?;
    let mut rev = lookup_rev(&repo, sha)?;
    Ok(Commit::from_git2_commit(&mut rev))
}

/// Returns the bors merge commits between the two specified boundaries
/// (boundaries inclusive).
pub fn get_commits_between(first_commit: &str, last_commit: &str) -> anyhow::Result<Vec<Commit>> {
    let repo = get_repo()?;
    eprintln!("looking up first commit");
    let mut first = lookup_rev(&repo, first_commit)?;
    eprintln!("looking up second commit");
    let last = lookup_rev(&repo, last_commit)?;

    // Sanity check -- our algorithm below only works reliably if the
    // two commits are merge commits made by bors
    let assert_by_bors = |c: &Git2Commit<'_>| -> anyhow::Result<()> {
        match c.author().name() {
            Some(author) if author == BORS_AUTHOR => Ok(()),
            Some(author) => bail!(
                "Expected author {author} to be {BORS_AUTHOR} for {}.\n \
                Make sure specified commits are on the default branch!",
                c.id()
            ),
            None => bail!("No author for {}", c.id()),
        }
    };

    eprintln!("checking that commits are by bors and thus have ci artifacts...");
    assert_by_bors(&first)?;
    assert_by_bors(&last)?;
    // Now find the commits
    // We search from the last and always take the first of its parents,
    // to only get merge commits.
    // This uses the fact that all bors merge commits have the earlier
    // merge commit as their first parent.
    eprintln!("finding bors merge commits");
    let mut res = Vec::new();
    let mut current = last;
    loop {
        assert_by_bors(&current)?;
        res.push(Commit::from_git2_commit(&mut current));
        match current.parents().next() {
            Some(c) => {
                if c.author().name() != Some(BORS_AUTHOR) {
                    debug!(
                        "{:?} has non-bors author: {:?}, skipping",
                        c.id(),
                        c.author().name()
                    );
                    current = c.parents().next().unwrap();
                    continue;
                }
                current = c;
                if current.id() == first.id() {
                    // Reached the first commit, our end of the search.
                    break;
                }
            }
            None => bail!("reached end of repo without encountering the first commit"),
        }
    }
    res.push(Commit::from_git2_commit(&mut first));
    // Reverse in order to obtain chronological order
    res.reverse();
    eprintln!(
        "found {} bors merge commits in the specified range",
        res.len()
    );
    Ok(res)
}

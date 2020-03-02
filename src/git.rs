// Copyright 2018 The Rust Project Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Get git commits with help of the libgit2 library

const RUST_SRC_URL: &str = "https://github.com/rust-lang/rust";
const RUST_SRC_REPO: Option<&str> = option_env!("RUST_SRC_REPO");

use std::path::Path;

use chrono::{TimeZone, Utc};
use failure::{bail, Error};
use git2::build::RepoBuilder;
use git2::{Commit as Git2Commit, Repository};
use log::debug;

use crate::Commit;

impl Commit {
    // Takes &mut because libgit2 internally caches summaries
    fn from_git2_commit(commit: &mut Git2Commit<'_>) -> Self {
        Commit {
            sha: commit.id().to_string(),
            date: Utc.timestamp(commit.time().seconds(), 0),
            summary: String::from_utf8_lossy(commit.summary_bytes().unwrap()).to_string(),
        }
    }
}

fn lookup_rev<'rev>(repo: &'rev Repository, rev: &str) -> Result<Git2Commit<'rev>, Error> {
    if let Ok(c) = repo.revparse_single(rev)?.into_commit() {
        return Ok(c);
    }
    bail!("Could not find a commit for revision specifier '{}'", rev)
}

fn get_repo() -> Result<Repository, Error> {
    let loc = Path::new("rust.git");
    match (RUST_SRC_REPO, loc.exists()) {
        (Some(_), _) | (_, true) => {
            let path = RUST_SRC_REPO.map(Path::new).unwrap_or(loc);
            eprintln!("opening existing repository at {:?}", path);
            let repo = Repository::open(path)?;
            {
                eprintln!("refreshing repository");
                let mut remote = repo.find_remote("origin")
                    .or_else(|_| repo.remote_anonymous("origin"))?;
                remote.fetch(&["master"], None, None)?;
            }
            Ok(repo)
        }
        (None, false) => {
            eprintln!("cloning rust repository");
            Ok(RepoBuilder::new()
                .bare(true)
                .clone(RUST_SRC_URL, Path::new("rust.git"))?)
        }
    }
}

pub fn expand_commit(sha: &str) -> Result<String, Error> {
    let repo = get_repo()?;
    let rev = lookup_rev(&repo, sha)?;
    Ok(rev.id().to_string())
}

/// Returns the bors merge commits between the two specified boundaries
/// (boundaries inclusive).
pub fn get_commits_between(first_commit: &str, last_commit: &str) -> Result<Vec<Commit>, Error> {
    let repo = get_repo()?;
    eprintln!("looking up first commit");
    let mut first = lookup_rev(&repo, first_commit)?;
    eprintln!("looking up second commit");
    let last = lookup_rev(&repo, last_commit)?;

    // Sanity check -- our algorithm below only works reliably if the
    // two commits are merge commits made by bors
    let assert_by_bors = |c: &Git2Commit<'_>| -> Result<(), Error> {
        match c.author().name() {
            Some("bors") => Ok(()),
            Some(author) => bail!("Expected author {} to be bors for {}", author, c.id()),
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
                if c.author().name() != Some("bors") {
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

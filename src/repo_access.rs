use anyhow::Context;

use crate::{Bound, Commit, Error, GitDate, git, github};

pub(crate) trait RustRepositoryAccessor {
    /// Maps `bound` to its associated date, looking up its commit if necessary.
    fn bound_to_date(&self, bound: Bound) -> Result<GitDate, Error> {
        match bound {
            Bound::Date(date) => Ok(date),
            Bound::Commit(ref commit_ref) => self.commit(commit_ref).map(|commit| commit.date),
        }
    }

    /// Looks up commit associated with `commit_ref`, which can be either a sha
    /// or a more general reference like "origin/master".
    fn commit(&self, commit_ref: &str) -> Result<Commit, Error>;

    /// Looks up a series of commits ending with `end_sha`; the resulting series
    /// should start with `start_sha`. If `start_sha` is not a predecessor of
    /// `end_sha` in the history, then the series will cover all commits as far
    /// back as the date associated with `start_sha`.
    fn commits(&self, start_sha: &str, end_sha: &str) -> Result<Vec<Commit>, Error>;
}

pub(crate) struct AccessViaLocalGit;

pub(crate) struct AccessViaGithub;

impl RustRepositoryAccessor for AccessViaLocalGit {
    fn commit(&self, commit_ref: &str) -> Result<Commit, Error> {
        git::get_commit(commit_ref)
    }
    fn commits(&self, start_sha: &str, end_sha: &str) -> Result<Vec<Commit>, Error> {
        let end_sha = if end_sha == "origin/master" {
            "FETCH_HEAD"
        } else {
            end_sha
        };
        eprintln!(
            "fetching (via local git) commits from {} to {}",
            start_sha, end_sha
        );
        git::get_commits_between(start_sha, end_sha)
            .context("failed during attempt to create/access local git repository")
    }
}

impl RustRepositoryAccessor for AccessViaGithub {
    fn commit(&self, commit_ref: &str) -> Result<Commit, Error> {
        github::get_commit(commit_ref)
    }

    fn commits(&self, start_sha: &str, end_sha: &str) -> Result<Vec<Commit>, Error> {
        // `earliest_date` is an lower bound on what we should search in our
        // github query. Why is it `start` date minus 1?
        //
        // Because: the "since" parameter in the github API is an exclusive
        // bound. We need an inclusive bound, so we go yet another day prior for
        // this bound on the github search.
        let since_date = self
            .bound_to_date(Bound::Commit(start_sha.to_string()))?
            .pred();

        eprintln!(
            "fetching (via remote github) commits from max({}, {}) to {}",
            start_sha,
            since_date.format(crate::YYYY_MM_DD),
            end_sha
        );

        let query = github::CommitsQuery {
            since_date: &since_date.format(crate::YYYY_MM_DD).to_string(),
            earliest_sha: start_sha,
            most_recent_sha: end_sha,
        };

        query.get_commits()
    }
}

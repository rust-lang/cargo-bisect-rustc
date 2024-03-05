//! Definitions of bisection bounds.

use crate::toolchains::{
    download_progress, parse_to_naive_date, Toolchain, NIGHTLY_SERVER, YYYY_MM_DD,
};
use crate::GitDate;
use crate::Opts;
use crate::{today, EPOCH_COMMIT};
use anyhow::bail;
use chrono::NaiveDate;
use reqwest::blocking::Client;
use std::io::Read;
use std::str::FromStr;

/// A bisection boundary.
#[derive(Clone, Debug)]
pub enum Bound {
    Commit(String),
    Date(GitDate),
}

impl FromStr for Bound {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_to_naive_date(s)
            .map(Self::Date)
            .or_else(|_| Ok(Self::Commit(s.to_string())))
    }
}

impl Bound {
    /// Returns the SHA of this boundary.
    ///
    /// For nightlies, this will fetch from the network.
    pub fn sha(&self) -> anyhow::Result<String> {
        match self {
            Bound::Commit(commit) => Ok(commit.clone()),
            Bound::Date(date) => {
                let date_str = date.format(YYYY_MM_DD);
                let url =
                    format!("{NIGHTLY_SERVER}/{date_str}/channel-rust-nightly-git-commit-hash.txt");

                eprintln!("fetching {url}");
                let client = Client::new();
                let name = format!("nightly manifest {date_str}");
                let mut response = download_progress(&client, &name, &url)?;
                let mut commit = String::new();
                response.read_to_string(&mut commit)?;

                eprintln!("converted {date_str} to {commit}");

                Ok(commit)
            }
        }
    }
}

/// The starting bisection bounds.
pub enum Bounds {
    /// Indicates to search backwards from the given date to find the start
    /// date where the regression does not occur.
    SearchNightlyBackwards { end: GitDate },
    /// Search between two commits.
    Commits { start: String, end: String },
    /// Search between two dates.
    Dates { start: GitDate, end: GitDate },
}

impl Bounds {
    pub fn from_args(args: &Opts) -> anyhow::Result<Bounds> {
        let (start, end) = translate_tags(&args)?;
        let today = today();
        let check_in_future = |which, date: &NaiveDate| -> anyhow::Result<()> {
            if date > &today {
                bail!(
                    "{which} date should be on or before current date, \
                     got {which} date request: {date} and current date is {today}"
                );
            }
            Ok(())
        };
        let bounds = match (start, end) {
            // Neither --start or --end specified.
            (None, None) => Bounds::SearchNightlyBackwards {
                end: installed_nightly_or_latest()?,
            },

            // --start or --end is a commit
            (Some(Bound::Commit(start)), Some(Bound::Commit(end))) => {
                Bounds::Commits { start, end }
            }
            (Some(Bound::Commit(start)), None) => Bounds::Commits {
                start,
                end: "origin/master".to_string(),
            },
            (None, Some(Bound::Commit(end))) => Bounds::Commits {
                start: EPOCH_COMMIT.to_string(),
                end,
            },

            // --start or --end is a date
            (Some(Bound::Date(start)), Some(Bound::Date(end))) => {
                check_in_future("start", &start)?;
                check_in_future("end", &end)?;
                Bounds::Dates { start, end }
            }
            (Some(Bound::Date(start)), None) => {
                check_in_future("start", &start)?;
                Bounds::Dates {
                    start,
                    end: find_latest_nightly()?,
                }
            }
            (None, Some(Bound::Date(end))) => {
                check_in_future("end", &end)?;
                if args.by_commit {
                    bail!("--by-commit with an end date requires --start to be specified");
                }
                Bounds::SearchNightlyBackwards { end }
            }

            // Mixed not supported.
            (Some(Bound::Commit(_)), Some(Bound::Date(_)))
            | (Some(Bound::Date(_)), Some(Bound::Commit(_))) => bail!(
                "cannot take different types of bounds for start/end, \
                 got start: {:?} and end {:?}",
                args.start,
                args.end
            ),
        };
        if let Bounds::Dates { start, end } = &bounds {
            if end < start {
                bail!("end should be after start, got start: {start} and end {end}");
            }
            if args.by_commit {
                eprintln!("finding commit range that corresponds to dates specified");
                let bounds = Bounds::Commits {
                    start: date_to_sha(&start)?,
                    end: date_to_sha(&end)?,
                };
                return Ok(bounds);
            }
        }
        Ok(bounds)
    }
}

/// Translates a tag-like bound (such as `1.62.0`) to a `Bound::Date` so that
/// bisecting works for versions older than 167 days.
fn translate_tags(args: &Opts) -> anyhow::Result<(Option<Bound>, Option<Bound>)> {
    let is_tag = |bound: &Option<Bound>| -> bool {
        match bound {
            Some(Bound::Commit(commit)) => commit.contains('.'),
            None | Some(Bound::Date(_)) => false,
        }
    };
    let is_datelike = |bound: &Option<Bound>| -> bool {
        matches!(bound, None | Some(Bound::Date(_))) || is_tag(bound)
    };
    if !(is_datelike(&args.start) && is_datelike(&args.end)) {
        // If the user specified an actual commit for one bound, then don't
        // even try to convert the other bound to a date.
        return Ok((args.start.clone(), args.end.clone()));
    }
    let fixup = |which: &str, bound: &Option<Bound>| -> anyhow::Result<Option<Bound>> {
        if is_tag(bound) {
            if let Some(Bound::Commit(tag)) = bound {
                let date = args
                    .access
                    .repo()
                    .bound_to_date(Bound::Commit(tag.clone()))?;
                eprintln!(
                    "translating --{which}={tag} to {date}",
                    date = date.format(YYYY_MM_DD)
                );
                return Ok(Some(Bound::Date(date)));
            }
        }
        Ok(bound.clone())
    };
    Ok((fixup("start", &args.start)?, fixup("end", &args.end)?))
}

/// Returns the commit SHA of the nightly associated with the given date.
fn date_to_sha(date: &NaiveDate) -> anyhow::Result<String> {
    let date_str = date.format(YYYY_MM_DD);
    let url = format!("{NIGHTLY_SERVER}/{date_str}/channel-rust-nightly-git-commit-hash.txt");

    eprintln!("fetching {url}");
    let client = Client::new();
    let name = format!("nightly manifest {date_str}");
    let mut response = download_progress(&client, &name, &url)?;
    let mut commit = String::new();
    response.read_to_string(&mut commit)?;

    eprintln!("converted {date_str} to {commit}");

    Ok(commit)
}

/// Returns the date of the nightly toolchain currently installed. If no
/// nightly is found, then it goes to the network to determine the date of the
/// latest nightly.
fn installed_nightly_or_latest() -> anyhow::Result<GitDate> {
    if let Some(date) = Toolchain::default_nightly() {
        return Ok(date);
    }
    find_latest_nightly()
}

/// Returns the date of the latest nightly (fetched from the network).
fn find_latest_nightly() -> anyhow::Result<GitDate> {
    let url = format!("{NIGHTLY_SERVER}/channel-rust-nightly.toml");
    eprintln!("fetching {url}");
    let client = Client::new();
    let mut response = download_progress(&client, "nightly manifest", &url)?;
    let mut manifest = String::new();
    response.read_to_string(&mut manifest)?;
    let manifest: toml::Value = toml::from_str(&manifest)?;
    let date = manifest["date"].as_str().expect("date is a string");
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
    eprintln!("determined the latest nightly is {date}");
    Ok(date)
}

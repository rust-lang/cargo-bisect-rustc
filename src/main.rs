// Copyright 2018 The Rust Project Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

extern crate chrono;
extern crate env_logger;
#[macro_use]
extern crate failure;
extern crate flate2;
extern crate git2;
#[macro_use]
extern crate log;
extern crate pbr;
#[cfg(test)]
extern crate quickcheck;
extern crate reqwest;
#[macro_use]
extern crate structopt;
extern crate tar;
extern crate tee;
extern crate tempdir;
extern crate xz2;

use std::fmt;
use std::fs;
use std::env;
use std::process::{self, Command};
use std::path::{Path, PathBuf};
use std::io::{self, Read, Write};
use std::ffi::OsString;
use std::str::FromStr;

use tempdir::TempDir;
use failure::Error;
use structopt::StructOpt;
use reqwest::{Client, Response};
use pbr::{ProgressBar, Units};
use tee::TeeReader;
use tar::Archive;
use reqwest::header::ContentLength;
use xz2::read::XzDecoder;
use flate2::read::GzDecoder;
use chrono::{Date, Duration, Utc};

/// The first commit which build artifacts are made available through the CI for
/// bisection.
///
/// Due to our deletion policy which expires builds after 90 days, the build
/// artifacts of this commit itself is no longer available, so this may not be entirely useful;
/// however, it does limit the amount of commits somewhat.
const EPOCH_COMMIT: &str = "927c55d86b0be44337f37cf5b0a76fb8ba86e06c";

const NIGHTLY_SERVER: &str = "https://static.rust-lang.org/dist";
const CI_SERVER: &str = "https://s3-us-west-1.amazonaws.com/rust-lang-ci2";

mod git;
mod least_satisfying;
use least_satisfying::{least_satisfying, Satisfies};

fn get_commits(start: &str, end: &str) -> Result<Vec<git::Commit>, Error> {
    let commits = git::get_commits_between(start, end)?;
    assert_eq!(commits.first().expect("at least one commit").sha, start);

    Ok(commits)
}

#[derive(Debug, StructOpt)]
struct Opts {
    #[structopt(short = "a", long = "alt",
                help = "Download the alt build instead of normal build")]
    alt: bool,

    #[structopt(long = "host", help = "Host triple for the compiler", default_value = "unknown")]
    host: String,

    #[structopt(long = "target", help = "Target platform to install for cross-compilation")]
    target: Option<String>,

    #[structopt(long = "preserve", help = "Preserve the downloaded artifacts")]
    preserve: bool,

    #[structopt(long = "with-cargo",
                help = "Download cargo, by default the installed cargo is used")]
    with_cargo: bool,

    #[structopt(long = "test-dir",
                help = "Directory to test; this is where you usually run `cargo build`",
                parse(from_os_str))]
    test_dir: PathBuf,

    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbosity: usize,

    #[structopt(help = "Arguments to pass to cargo when running",
                raw(multiple = "true", last = "true"), parse(from_os_str))]
    cargo_args: Vec<OsString>,

    #[structopt(long = "start",
                help = "the left-bound for the search; this point should *not* pass the test")]
    start: Option<Bound>,

    #[structopt(long = "end",
                help = "the right-bound for the search; this point should pass the test")]
    end: Option<Bound>,

    #[structopt(long = "by-commit",
                help = "without specifying bounds, bisect via commit artifacts")]
    by_commit: bool,
}

#[derive(Clone, Debug)]
enum Bound {
    Commit(String),
    Date(Date<Utc>),
}

#[derive(Fail, Debug)]
#[fail(display = "will never happen")]
struct BoundParseError {}

impl FromStr for Bound {
    type Err = BoundParseError;
    fn from_str(s: &str) -> Result<Bound, BoundParseError> {
        match chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            Ok(date) => Ok(Bound::Date(Date::from_utc(date, Utc))),
            Err(_) => Ok(Bound::Commit(s.to_string())),
        }
    }
}

impl Opts {
    fn emit_cargo_output(&self) -> bool {
        self.verbosity >= 2
    }
}

#[derive(Debug, Fail)]
struct ExitError(i32);

impl fmt::Display for ExitError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "exiting with {}", self.0)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct Toolchain {
    spec: ToolchainSpec,
    host: String,
    std_targets: Vec<String>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum ToolchainSpec {
    Ci { commit: String, alt: bool },
    Nightly { date: Date<Utc> },
}

impl fmt::Display for ToolchainSpec {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ToolchainSpec::Ci { ref commit, alt } => {
                let alt_s = if alt { format!("-alt") } else { String::new() };
                write!(f, "{}{}", commit, alt_s)
            }
            ToolchainSpec::Nightly { ref date } => write!(f, "nightly-{}", date),
        }
    }
}

impl Toolchain {
    fn rustup_name(&self) -> String {
        match self.spec {
            ToolchainSpec::Ci { ref commit, alt } => {
                let alt_s = if alt { format!("-alt") } else { String::new() };
                format!("ci-{}{}-{}", commit, alt_s, self.host)
            }
            // N.B. We need to call this with a nonstandard name so that rustup utilizes the
            // fallback cargo logic.
            ToolchainSpec::Nightly { ref date } => {
                format!("bisector-nightly-{}-{}", date.format("%Y-%m-%d"), self.host)
            }
        }
    }
}

impl fmt::Display for Toolchain {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.spec {
            ToolchainSpec::Ci { ref commit, alt } => {
                let alt_s = if alt { format!("-alt") } else { String::new() };
                write!(f, "{}{}", commit, alt_s)
            }
            ToolchainSpec::Nightly { ref date } => write!(f, "nightly-{}", date.format("%Y-%m-%d")),
        }
    }
}

#[derive(Clone, Debug)]
struct DownloadParams {
    url_prefix: String,
    install_dir: PathBuf,
    install_cargo: bool,
}

#[derive(Fail, Debug)]
enum ArchiveError {
    #[fail(display = "Failed to parse archive: {}", _0)]
    Archive(#[cause] io::Error),
    #[fail(display = "Failed to create directory: {}", _0)]
    CreateDir(#[cause] io::Error),
}

#[derive(Fail, Debug)]
enum DownloadError {
    #[fail(display = "Tarball not found at {}", _0)]
    NotFound(String),
    #[fail(display = "A reqwest error occurred: {}", _0)]
    Reqwest(#[cause] reqwest::Error),
    #[fail(display = "An archive error occurred: {}", _0)]
    Archive(#[cause] ArchiveError),
}

fn download_progress(
    client: &Client,
    name: &str,
    url: &str,
) -> Result<(Response, ProgressBar<io::Stdout>), DownloadError> {
    debug!("downloading <{}>...", url);

    let response = client.get(url).send().map_err(DownloadError::Reqwest)?;

    if response.status() == reqwest::StatusCode::NotFound {
        return Err(DownloadError::NotFound(url.to_string()));
    }
    let response = response.error_for_status().map_err(DownloadError::Reqwest)?;

    let length = response
        .headers()
        .get::<ContentLength>()
        .map(|h| h.0)
        .unwrap_or(0);
    let mut bar = ProgressBar::new(length);
    bar.set_units(Units::Bytes);
    bar.message(&format!("{}: ", name));

    Ok((response, bar))
}

fn download_tar_xz(
    client: &Client,
    name: &str,
    url: &str,
    strip_prefix: Option<&Path>,
    dest: &Path,
) -> Result<(), DownloadError> {
    let (response, mut bar) = download_progress(client, name, url)?;
    let response = TeeReader::new(response, &mut bar);
    let response = XzDecoder::new(response);
    unarchive(response, strip_prefix, dest).map_err(DownloadError::Archive)?;
    Ok(())
}

fn download_tar_gz(
    client: &Client,
    name: &str,
    url: &str,
    strip_prefix: Option<&Path>,
    dest: &Path,
) -> Result<(), DownloadError> {
    let (response, mut bar) = download_progress(client, name, url)?;
    let response = TeeReader::new(response, &mut bar);
    let response = GzDecoder::new(response);
    unarchive(response, strip_prefix, dest).map_err(DownloadError::Archive)?;
    Ok(())
}

fn unarchive<R: Read>(r: R, strip_prefix: Option<&Path>, dest: &Path) -> Result<(), ArchiveError> {
    for entry in Archive::new(r).entries().map_err(ArchiveError::Archive)? {
        let mut entry = entry.map_err(ArchiveError::Archive)?;
        let dest_path = {
            let path = entry.path().map_err(ArchiveError::Archive)?;
            let sub_path = match strip_prefix {
                Some(prefix) => path.strip_prefix(prefix).map(PathBuf::from),
                None => Ok(path.into_owned()),
            };
            match sub_path {
                Ok(sub_path) => dest.join(sub_path),
                Err(_) => continue,
            }
        };
        fs::create_dir_all(dest_path.parent().unwrap()).map_err(ArchiveError::CreateDir)?;
        entry.unpack(dest_path).map_err(ArchiveError::Archive)?;
    }

    Ok(())
}

fn download_tarball(
    client: &Client,
    name: &str,
    url: &str,
    strip_prefix: Option<&Path>,
    dest: &Path,
) -> Result<(), DownloadError> {
    match download_tar_xz(client, name, &format!("{}.xz", url,), strip_prefix, dest) {
        Ok(()) => return Ok(()),
        Err(DownloadError::NotFound { .. }) => {}
        Err(e) => return Err(e),
    }
    download_tar_gz(client, name, &format!("{}.gz", url,), strip_prefix, dest)
}

#[derive(Fail, Debug)]
enum InstallError {
    #[fail(display = "Could not find {}; url: {}", spec, url)]
    NotFound { url: String, spec: ToolchainSpec },
    #[fail(display = "Could not download toolchain: {}", _0)]
    Download(#[cause] DownloadError),
    #[fail(display = "Could not create tempdir: {}", _0)]
    TempDir(#[cause] io::Error),
    #[fail(display = "Could not move tempdir into destination: {}", _0)]
    Move(#[cause] io::Error),
}

impl Toolchain {
    fn remove(&self, dl_params: &DownloadParams) -> Result<(), Error> {
        eprintln!("uninstalling {}", self);
        let dir = dl_params.install_dir.join(self.rustup_name());
        fs::remove_dir_all(&dir)?;
        Ok(())
    }

    fn test(&self, cfg: &Config, dl_spec: &DownloadParams) -> process::ExitStatus {
        // do things with this toolchain
        let mut cargo = Command::new("cargo");
        cargo.arg(&format!("+{}", self.rustup_name()));
        cargo.current_dir(&cfg.args.test_dir);
        if cfg.args.cargo_args.is_empty() {
            cargo.arg("build");
        } else {
            cargo.args(&cfg.args.cargo_args);
        }
        let output = match cargo.output() {
            Ok(output) => output,
            Err(err) => {
                panic!("failed to run {:?}: {:?}", cargo, err);
            }
        };
        if cfg.args.emit_cargo_output() {
            io::stderr().write_all(&output.stderr).unwrap();
            io::stdout().write_all(&output.stdout).unwrap();
        }
        if !cfg.args.preserve {
            let _ = self.remove(dl_spec);
        }

        output.status
    }

    fn install(&self, client: &Client, dl_params: &DownloadParams) -> Result<(), InstallError> {
        debug!("installing {}", self);
        let tmpdir = TempDir::new(&self.rustup_name()).map_err(InstallError::TempDir)?;
        let dest = dl_params.install_dir.join(self.rustup_name());
        if dest.is_dir() {
            // already installed
            return Ok(());
        }

        let rustc_filename = format!("rustc-nightly-{}", self.host);

        let location = match self.spec {
            ToolchainSpec::Ci { ref commit, .. } => commit.to_string(),
            ToolchainSpec::Nightly { ref date } => date.format("%Y-%m-%d").to_string(),
        };

        // download rustc.
        if let Err(e) = download_tarball(
            &client,
            &format!("rustc for {}", self.host),
            &format!(
                "{}/{}/{}.tar",
                dl_params.url_prefix, location, rustc_filename
            ),
            Some(&PathBuf::from(&rustc_filename).join("rustc")),
            tmpdir.path(),
        ) {
            match e {
                DownloadError::NotFound(url) => {
                    return Err(InstallError::NotFound {
                        url: url,
                        spec: self.spec.clone(),
                    })
                }
                _ => return Err(InstallError::Download(e)),
            }
        }

        // download libstd.
        for target in &self.std_targets {
            let rust_std_filename = format!("rust-std-nightly-{}", target);
            download_tarball(
                &client,
                &format!("std for {}", target),
                &format!(
                    "{}/{}/{}.tar",
                    dl_params.url_prefix, location, rust_std_filename
                ),
                Some(&PathBuf::from(&rust_std_filename)
                    .join(format!("rust-std-{}", target))
                    .join("lib")),
                &tmpdir.path().join("lib"),
            ).map_err(InstallError::Download)?;
        }

        if dl_params.install_cargo {
            let filename = format!("cargo-nightly-{}", self.host);
            download_tarball(
                &client,
                &format!("cargo for {}", self.host),
                &format!("{}/{}/{}.tar", dl_params.url_prefix, location, filename,),
                Some(&PathBuf::from(&filename).join("cargo")),
                tmpdir.path(),
            ).map_err(InstallError::Download)?;
        }

        fs::rename(tmpdir.into_path(), dest).map_err(InstallError::Move)?;

        Ok(())
    }
}

struct Config {
    args: Opts,
    toolchains_path: PathBuf,
    target: String,
    is_commit: bool,
}

impl Config {
    fn from_args(mut args: Opts) -> Result<Config, Error> {
        if args.host == "unknown" {
            if let Some(host) = option_env!("HOST") {
                args.host = host.to_string();
            } else {
                bail!(
                    "Failed to auto-detect host triple and was not specified. Please provide it via --host"
                );
            }
        }

        let target = args.target.clone().unwrap_or_else(|| args.host.clone());
        let args = args;

        let mut toolchains_path = match env::var_os("RUSTUP_HOME") {
            Some(h) => PathBuf::from(h),
            None => {
                let mut home = env::home_dir().ok_or_else(|| format_err!("Could not find home."))?;
                home.push(".rustup");
                home
            }
        };
        toolchains_path.push("toolchains");
        if !toolchains_path.is_dir() {
            bail!(
                "`{}` is not a directory. Please install rustup.",
                toolchains_path.display()
            );
        }

        let is_commit = match (args.start.clone(), args.end.clone()) {
            (Some(Bound::Commit(_)), Some(Bound::Commit(_)))
            | (None, Some(Bound::Commit(_)))
            | (Some(Bound::Commit(_)), None) => Some(true),

            (Some(Bound::Date(_)), Some(Bound::Date(_)))
            | (None, Some(Bound::Date(_)))
            | (Some(Bound::Date(_)), None) => Some(false),

            (None, None) => None,

            (start, end) => bail!(
                "cannot take different types of bounds for start/end, got start: {:?} and end {:?}",
                start,
                end
            ),
        };

        if is_commit == Some(false) && args.by_commit {
            // FIXME: In theory, we could use the date range to narrow down the commit list...
            bail!("cannot bisect by-commit if specifying date range");
        }

        Ok(Config {
            is_commit: args.by_commit || is_commit == Some(true),
            args,
            target,
            toolchains_path,
        })
    }
}

fn run() -> Result<(), Error> {
    env_logger::try_init()?;
    let args = env::args_os().filter(|a| a != "bisect-rustc");
    let args = Opts::from_iter(args);
    let cfg = Config::from_args(args)?;

    let client = Client::new();

    let BisectionResult {
        searched: toolchains,
        dl_spec,
        found,
    } = if cfg.is_commit {
        bisect_ci(&cfg, &client)?
    } else {
        bisect_nightlies(&cfg, &client)?
    };

    eprintln!(
        "searched toolchains {} through {}",
        toolchains.first().unwrap(),
        toolchains.last().unwrap(),
    );

    if toolchains[found] == *toolchains.last().unwrap() {
        let t = &toolchains[found];
        let r = match t.install(&client, &dl_spec) {
            Ok(()) => {
                let status = t.test(&cfg, &dl_spec);
                if !cfg.args.preserve {
                    let _ = t.remove(&dl_spec);
                }
                // we want to fail, so a successful build doesn't satisfy us
                if status.success() {
                    Satisfies::No
                } else {
                    Satisfies::Yes
                }
            }
            Err(_) => {
                let _ = t.remove(&dl_spec);
                Satisfies::Unknown
            }
        };
        match r {
            Satisfies::Yes => {}
            Satisfies::No | Satisfies::Unknown => {
                eprintln!("error: The regression was not found. Expanding the bounds may help.");
                return Ok(());
            }
        }
    }

    eprintln!("regression in {}", toolchains[found]);

    Ok(())
}

fn bisect_nightlies(cfg: &Config, client: &Client) -> Result<BisectionResult, Error> {
    if cfg.args.alt {
        bail!("cannot bisect nightlies with --alt: not supported");
    }

    let dl_spec = DownloadParams {
        url_prefix: NIGHTLY_SERVER.to_string(),
        install_dir: cfg.toolchains_path.clone(),
        install_cargo: cfg.args.with_cargo,
    };

    let now = chrono::Utc::now();
    let today = now.date();
    let mut nightly_date = today;
    let mut jump_length = 1;
    // before this date we didn't have -std packages
    let end_at = chrono::Date::from_utc(
        chrono::naive::NaiveDate::from_ymd(2015, 10, 20),
        chrono::Utc,
    );
    let mut first_success = None;
    let mut last_failure = today;
    while nightly_date > end_at {
        let mut t = Toolchain {
            spec: ToolchainSpec::Nightly { date: nightly_date },
            host: cfg.args.host.clone(),
            std_targets: vec![cfg.args.host.clone(), cfg.target.clone()],
        };
        t.std_targets.sort();
        t.std_targets.dedup();
        eprintln!("checking {}", t);
        match t.install(client, &dl_spec) {
            Ok(()) => {
                let status = t.test(&cfg, &dl_spec);
                if status.success() {
                    first_success = Some(nightly_date);
                    break;
                } else {
                    last_failure = nightly_date;
                }
                nightly_date = nightly_date - chrono::Duration::days(jump_length);
                jump_length *= 2;
                if !cfg.args.preserve {
                    let _ = t.remove(&dl_spec);
                }
            }
            Err(InstallError::NotFound { .. }) => {
                // go back just one day, presumably missing nightly
                nightly_date = nightly_date - chrono::Duration::days(1);
                if !cfg.args.preserve {
                    let _ = t.remove(&dl_spec);
                }
            }
            Err(e) => {
                if !cfg.args.preserve {
                    let _ = t.remove(&dl_spec);
                }
                return Err(e)?;
            }
        }
    }

    let first_success = first_success.ok_or(format_err!("could not find a nightly that built"))?;

    let toolchains = toolchains_between(
        cfg,
        ToolchainSpec::Nightly {
            date: first_success,
        },
        ToolchainSpec::Nightly { date: last_failure },
    );

    let found = least_satisfying(&toolchains, |t| {
        match t.install(&client, &dl_spec) {
            Ok(()) => {
                let status = t.test(&cfg, &dl_spec);
                // we want to fail, so a successful build doesn't satisfy us
                let r = if status.success() {
                    Satisfies::No
                } else {
                    Satisfies::Yes
                };
                if !cfg.args.preserve {
                    let _ = t.remove(&dl_spec);
                }
                eprintln!("tested {}, got {}", t, r);
                r
            }
            Err(err) => {
                let _ = t.remove(&dl_spec);
                eprintln!("failed to install {}: {:?}", t, err);
                Satisfies::Unknown
            }
        }
    });

    Ok(BisectionResult {
        dl_spec,
        searched: toolchains,
        found,
    })
}

fn toolchains_between(cfg: &Config, a: ToolchainSpec, b: ToolchainSpec) -> Vec<Toolchain> {
    match (a, b) {
        (ToolchainSpec::Nightly { date: a }, ToolchainSpec::Nightly { date: b }) => {
            let mut toolchains = Vec::new();
            let mut date = a;
            while date <= b {
                let mut t = Toolchain {
                    spec: ToolchainSpec::Nightly { date: date },
                    host: cfg.args.host.clone(),
                    std_targets: vec![cfg.args.host.clone(), cfg.target.clone()],
                };
                t.std_targets.sort();
                t.std_targets.dedup();
                toolchains.push(t);
                date = date + Duration::days(1);
            }
            toolchains
        }
        _ => unimplemented!(),
    }
}

fn bisect_ci(cfg: &Config, client: &Client) -> Result<BisectionResult, Error> {
    let url_prefix = format!(
        "{}/rustc-builds{}",
        CI_SERVER,
        if cfg.args.alt { "-alt" } else { "" }
    );

    let dl_spec = DownloadParams {
        url_prefix: url_prefix,
        install_dir: cfg.toolchains_path.clone(),
        install_cargo: cfg.args.with_cargo,
    };

    let start = if let Some(Bound::Commit(ref sha)) = cfg.args.start {
        sha
    } else {
        EPOCH_COMMIT
    };

    let end = if let Some(Bound::Commit(ref sha)) = cfg.args.end {
        sha
    } else {
        "origin/master"
    };

    let mut commits = get_commits(start, end)?;
    let now = chrono::Utc::now();
    commits.retain(|c| now.signed_duration_since(c.date).num_days() < 90);

    if commits.is_empty() {
        bail!(
            "no commits between {} and {} within last 90 days",
            start,
            end
        );
    }

    if let Some(ref c) = commits.first() {
        if !c.sha.starts_with(start) {
            bail!(
                "expected to start with {}, but started with {}",
                start,
                c.sha
            );
        }
    }

    if let Some(ref c) = commits.last() {
        if end != "origin/master" && !c.sha.starts_with(end) {
            bail!("expected to end with {}, but ended with {}", end, c.sha);
        }
    }

    let toolchains = commits
        .into_iter()
        .map(|commit| {
            let mut t = Toolchain {
                spec: ToolchainSpec::Ci {
                    commit: commit.sha.clone(),
                    alt: cfg.args.alt,
                },
                host: cfg.args.host.clone(),
                std_targets: vec![cfg.args.host.clone(), cfg.target.clone()],
            };
            t.std_targets.sort();
            t.std_targets.dedup();
            t
        })
        .collect::<Vec<_>>();

    let found = least_satisfying(&toolchains, |t| {
        match t.install(&client, &dl_spec) {
            Ok(()) => {
                let status = t.test(&cfg, &dl_spec);
                // we want to fail, so a successful build doesn't satisfy us
                let r = if status.success() {
                    Satisfies::No
                } else {
                    Satisfies::Yes
                };
                eprintln!("tested {}, got {}", t, r);
                if !cfg.args.preserve {
                    let _ = t.remove(&dl_spec);
                }
                r
            }
            Err(err) => {
                let _ = t.remove(&dl_spec);
                eprintln!("failed to install {}: {:?}", t, err);
                Satisfies::Unknown
            }
        }
    });

    Ok(BisectionResult {
        searched: toolchains,
        found,
        dl_spec,
    })
}

#[derive(Clone)]
struct BisectionResult {
    searched: Vec<Toolchain>,
    found: usize,
    dl_spec: DownloadParams,
}

fn main() {
    if let Err(err) = run() {
        match err.downcast::<ExitError>() {
            Ok(ExitError(code)) => process::exit(code),
            Err(err) => {
                eprintln!("{}", err);
                process::exit(1);
            }
        }
    }
}

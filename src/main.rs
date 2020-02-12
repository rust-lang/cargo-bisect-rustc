// Copyright 2018 The Rust Project Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

extern crate chrono;
extern crate dialoguer;
extern crate dirs;
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
extern crate regex;
extern crate reqwest;
extern crate rustc_version;
extern crate structopt;
extern crate tar;
extern crate tee;
extern crate tempdir;
extern crate xz2;

use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::str::FromStr;

use chrono::{Date, Duration, naive, Utc};
use dialoguer::Select;
use failure::Error;
use flate2::read::GzDecoder;
use pbr::{ProgressBar, Units};
use regex::Regex;
use reqwest::header::CONTENT_LENGTH;
use reqwest::{Client, Response};
use rustc_version::Channel;
use structopt::StructOpt;
use tar::Archive;
use tee::TeeReader;
use tempdir::TempDir;
use xz2::read::XzDecoder;

/// The first commit which build artifacts are made available through the CI for
/// bisection.
///
/// Due to our deletion policy which expires builds after 167 days, the build
/// artifacts of this commit itself is no longer available, so this may not be entirely useful;
/// however, it does limit the amount of commits somewhat.
const EPOCH_COMMIT: &str = "927c55d86b0be44337f37cf5b0a76fb8ba86e06c";

const NIGHTLY_SERVER: &str = "https://static.rust-lang.org/dist";
const CI_SERVER: &str = "https://s3-us-west-1.amazonaws.com/rust-lang-ci2";

mod errors;
mod git;
mod least_satisfying;
use least_satisfying::{least_satisfying, Satisfies};
use errors::{
    ArchiveError,
    DownloadError,
    ExitStatusError,
    InstallError,
    NeverError,
};

fn get_commits(start: &str, end: &str) -> Result<Vec<git::Commit>, Error> {
    eprintln!("fetching commits from {} to {}", start, end);
    let commits = git::get_commits_between(start, end)?;
    assert_eq!(commits.first().expect("at least one commit").sha, git::expand_commit(start)?);

    Ok(commits)
}

#[derive(Debug, StructOpt)]
#[structopt(after_help = "EXAMPLES:
    Run a fully automatic nightly bisect doing `cargo check`:
    ```
    cargo bisect-rustc --start 2018-07-07 --end 2018-07-30 --test-dir ../my_project/ -- check
    ```

    Run a PR-based bisect with manual prompts after each run doing `cargo build`:
    ```
    cargo bisect-rustc --start 6a1c0637ce44aeea6c60527f4c0e7fb33f2bcd0d \\
      --end 866a713258915e6cbb212d135f751a6a8c9e1c0a --test-dir ../my_project/ --prompt -- build
    ```")]
struct Opts {
    #[structopt(
        long = "regress",
        default_value = "error",
        help = "Customize what is treated as regression.",
        long_help = "Customize what is treated as regression. \
                     Values include `--regress=error`, `--regress=non-error`, \
                     `--regress=ice`, and `--regress=success`."
    )]
    regress: String,

    #[structopt(
        short = "a", long = "alt", help = "Download the alt build instead of normal build"
    )]
    alt: bool,

    #[structopt(long = "host", help = "Host triple for the compiler", default_value = "unknown")]
    host: String,

    #[structopt(long = "target", help = "Target platform to install for cross-compilation")]
    target: Option<String>,

    #[structopt(long = "preserve", help = "Preserve the downloaded artifacts")]
    preserve: bool,

    #[structopt(long = "preserve-target", help = "Preserve the target directory used for builds")]
    preserve_target: bool,

    #[structopt(
        long = "with-cargo", help = "Download cargo, by default the installed cargo is used"
    )]
    with_cargo: bool,

    #[structopt(
        long = "with-src", help = "Download rust-src, by default this is not downloaded"
    )]
    with_src: bool,

    #[structopt(
        long = "test-dir",
        help = "Directory to test; this is where you usually run `cargo build`",
        default_value = ".",
        parse(from_os_str)
    )]
    test_dir: PathBuf,

    #[structopt(
        long = "prompt",
        help = "Display a prompt in between runs to allow for manually \
                inspecting output and retrying."
    )]
    prompt: bool,

    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbosity: usize,

    #[structopt(
        help = "Arguments to pass to cargo when running",
        raw(multiple = "true", last = "true"),
        parse(from_os_str)
    )]
    cargo_args: Vec<OsString>,

    #[structopt(
        long = "start",
        help = "the left-bound for the search; this point should *not* have the regression"
    )]
    start: Option<Bound>,

    #[structopt(
        long = "end", help = "the right-bound for the search; this point should have the regression"
    )]
    end: Option<Bound>,

    #[structopt(
        long = "by-commit", help = "without specifying bounds, bisect via commit artifacts"
    )]
    by_commit: bool,

    #[structopt(long = "install", help = "install the given artifact")]
    install: Option<Bound>,

    #[structopt(long = "force-install", help = "force installation over existing artifacts")]
    force_install: bool,

    #[structopt(
        long = "script",
        help = "script to run instead of cargo to test for regression",
        parse(from_os_str)
    )]
    script: Option<PathBuf>,
}

#[derive(Clone, Debug)]
enum Bound {
    Commit(String),
    Date(Date<Utc>),
}

impl FromStr for Bound {
    type Err = NeverError;
    fn from_str(s: &str) -> Result<Bound, NeverError> {
        match chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            Ok(date) => Ok(Bound::Date(Date::from_utc(date, Utc))),
            Err(_) => Ok(Bound::Commit(s.to_string())),
        }
    }
}

impl Bound {
    fn as_commit(self) -> Result<Self, Error> {
        match self {
            Bound::Commit(commit) => Ok(Bound::Commit(commit)),
            Bound::Date(date) => {
                let date_str = date.format("%Y-%m-%d");
                let url = format!("{}/{}/channel-rust-nightly-git-commit-hash.txt", NIGHTLY_SERVER, date_str);

                eprintln!("fetching {}", url);
                let client = Client::new();
                let name = format!("nightly manifest {}", date_str);
                let (response, mut bar) = download_progress(&client, &name, &url)?;
                let mut response = TeeReader::new(response, &mut bar);
                let mut commit = String::new();
                response.read_to_string(&mut commit)?;

                eprintln!("converted {} to {}", date_str, commit);

                Ok(Bound::Commit(commit))
            }
        }
    }
}

impl Opts {
    fn emit_cargo_output(&self) -> bool {
        self.verbosity >= 2
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
    tmp_dir: PathBuf,
    install_dir: PathBuf,
    install_cargo: bool,
    install_src: bool,
    force_install: bool,
}

impl DownloadParams {
    fn for_ci(cfg: &Config) -> Self {
        let url_prefix = format!(
            "{}/rustc-builds{}",
            CI_SERVER,
            if cfg.args.alt { "-alt" } else { "" }
        );

        DownloadParams {
            url_prefix: url_prefix,
            tmp_dir: cfg.rustup_tmp_path.clone(),
            install_dir: cfg.toolchains_path.clone(),
            install_cargo: cfg.args.with_cargo,
            install_src: cfg.args.with_src,
            force_install: cfg.args.force_install,
        }
    }

    fn for_nightly(cfg: &Config) -> Self {
        DownloadParams {
            url_prefix: NIGHTLY_SERVER.to_string(),
            tmp_dir: cfg.rustup_tmp_path.clone(),
            install_dir: cfg.toolchains_path.clone(),
            install_cargo: cfg.args.with_cargo,
            install_src: cfg.args.with_src,
            force_install: cfg.args.force_install,
        }
    }
}

fn download_progress(
    client: &Client,
    name: &str,
    url: &str,
) -> Result<(Response, ProgressBar<io::Stdout>), DownloadError> {
    debug!("downloading <{}>...", url);

    let response = client.get(url).send().map_err(DownloadError::Reqwest)?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(DownloadError::NotFound(url.to_string()));
    }
    let response = response.error_for_status().map_err(DownloadError::Reqwest)?;

    let length = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|c| c.to_str().ok())
        .and_then(|c| c.parse().ok())
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

#[derive(Debug)]
enum TestOutcome {
    Baseline,
    Regressed,
}

impl Toolchain {
    /// This returns the date of the default toolchain, if it is a nightly toolchain.
    /// Returns `None` if the installed toolchain is not a nightly toolchain.
    fn default_nightly() -> Option<Date<Utc>> {
        let version_meta = rustc_version::version_meta().unwrap();

        if let Channel::Nightly = version_meta.channel {
            if let Some(str_date) = version_meta.commit_date {
                let regex = Regex::new(r"(?m)^(\d{4})-(\d{2})-(\d{2})$").unwrap();
                if let Some(cap) = regex.captures(&str_date) {
                    let year = cap.get(1)?.as_str().parse::<i32>().ok()?;
                    let month = cap.get(2)?.as_str().parse::<u32>().ok()?;
                    let day = cap.get(3)?.as_str().parse::<u32>().ok()?;

                    return Some(Date::from_utc(
                        naive::NaiveDate::from_ymd(year, month, day),
                        Utc,
                    ));
                }
            }
        }

        None
    }

    fn is_current_nightly(&self) -> bool {
        if let ToolchainSpec::Nightly { date } = self.spec {
            if let Some(default_date) = Self::default_nightly() {
                return default_date == date;
            }
        }

        false
    }

    fn remove(&self, dl_params: &DownloadParams) -> Result<(), Error> {
        if !self.is_current_nightly() {
            eprintln!("uninstalling {}", self);
            let dir = dl_params.install_dir.join(self.rustup_name());
            fs::remove_dir_all(&dir)?;
        }

        Ok(())
    }

    fn test(&self, cfg: &Config) -> TestOutcome {
        let outcome = if cfg.args.prompt {
            loop {
                let output = self.run_test(cfg);
                let status = output.status;

                eprintln!("\n\n{} finished with exit code {:?}.", self, status.code());
                eprintln!("please select an action to take:");

                let default_choice = match cfg.default_outcome_of_output(output) {
                    TestOutcome::Regressed => 0,
                    TestOutcome::Baseline => 1,
                };

                match Select::new()
                    .items(&["mark regressed", "mark baseline", "retry"])
                    .default(default_choice)
                    .interact()
                    .unwrap()
                {
                    0 => break TestOutcome::Regressed,
                    1 => break TestOutcome::Baseline,
                    2 => continue,
                    _ => unreachable!(),
                }
            }
        } else {
            let output = self.run_test(cfg);
            cfg.default_outcome_of_output(output)
        };

        outcome
    }
}

impl Config {
    fn default_outcome_of_output(&self, output: process::Output) -> TestOutcome {
        let status = output.status;
        let stdout_utf8 = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr_utf8 = String::from_utf8_lossy(&output.stderr).to_string();

        debug!("status: {:?} stdout: {:?} stderr: {:?}", status, stdout_utf8, stderr_utf8);

        let saw_ice = || -> bool {
            stderr_utf8.contains("error: internal compiler error")
        };

        let input = (self.output_processing_mode(), status.success());
        let result = match input {
            (OutputProcessingMode::RegressOnErrorStatus, true) => TestOutcome::Baseline,
            (OutputProcessingMode::RegressOnErrorStatus, false) => TestOutcome::Regressed,

            (OutputProcessingMode::RegressOnSuccessStatus, true) => TestOutcome::Regressed,
            (OutputProcessingMode::RegressOnSuccessStatus, false) => TestOutcome::Baseline,

            (OutputProcessingMode::RegressOnIceAlone, _) => {
                if saw_ice() { TestOutcome::Regressed } else { TestOutcome::Baseline }
            }

            (OutputProcessingMode::RegressOnNonCleanError, true) => TestOutcome::Regressed,
            (OutputProcessingMode::RegressOnNonCleanError, false) => {
                if saw_ice() { TestOutcome::Regressed } else { TestOutcome::Baseline }
            }
        };
        debug!("default_outcome_of_output: input: {:?} result: {:?}", input, result);
        result
    }

    fn output_processing_mode(&self) -> OutputProcessingMode {
        match self.args.regress.as_str() {
            "error" => OutputProcessingMode::RegressOnErrorStatus,
            "non-error" => OutputProcessingMode::RegressOnNonCleanError,
            "ice" => OutputProcessingMode::RegressOnIceAlone,
            "success" => OutputProcessingMode::RegressOnSuccessStatus,
            setting => panic!("Unknown --regress setting: {:?}", setting),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, StructOpt)]
/// Customize what is treated as regression.
enum OutputProcessingMode {
    /// `RegressOnErrorStatus`: Marks test outcome as `Regressed` if and only if
    /// the `rustc` process reports a non-success status. This corresponds to
    /// when `rustc` has an internal compiler error (ICE) or when it detects an
    /// error in the input program.
    ///
    /// This covers the most common use case for `cargo-bisect-rustc` and is
    /// thus the default setting.
    ///
    /// You explicitly opt into this seting via `--regress=error`.
    RegressOnErrorStatus,

    /// `RegressOnSuccessStatus`: Marks test outcome as `Regressed` if and only
    /// if the `rustc` process reports a success status. This corresponds to
    /// when `rustc` believes it has successfully compiled the program. This
    /// covers the use case for when you want to bisect to see when a bug was
    /// fixed.
    ///
    /// You explicitly opt into this seting via `--regress=success`.
    RegressOnSuccessStatus,

    /// `RegressOnIceAlone`: Marks test outcome as `Regressed` if and only if
    /// the `rustc` process issues a diagnostic indicating that an internal
    /// compiler error (ICE) occurred. This covers the use case for when you
    /// want to bisect to see when an ICE was introduced pon a codebase that is
    /// meant to produce a clean error.
    ///
    /// You explicitly opt into this seting via `--regress=ice`.
    RegressOnIceAlone,

    /// `RegressOnNonCleanError`: Marks test outcome as `Baseline` if and only
    /// if the `rustc` process reports error status and does not issue any
    /// diagnostic indicating that an internal compiler error (ICE) occurred.
    /// This is the use case if the regression is a case where an ill-formed
    /// program has stopped being properly rejected by the compiler.
    ///
    /// (The main difference between this case and `RegressOnSuccessStatus` is
    /// the handling of ICE: `RegressOnSuccessStatus` assumes that ICE should be
    /// considered baseline; `RegressOnNonCleanError` assumes ICE should be
    /// considered a sign of a regression.)
    ///
    /// You explicitly opt into this seting via `--regress=non-error`.
    RegressOnNonCleanError,

}

impl OutputProcessingMode {
    fn must_process_stderr(&self) -> bool {
        match self {
            OutputProcessingMode::RegressOnErrorStatus |
            OutputProcessingMode::RegressOnSuccessStatus => false,

            OutputProcessingMode::RegressOnNonCleanError |
            OutputProcessingMode::RegressOnIceAlone => true,
        }
    }
}

impl Toolchain {
    fn run_test(&self, cfg: &Config) -> process::Output {
        if !cfg.args.preserve_target {
            let _ = fs::remove_dir_all(
                cfg.args
                    .test_dir
                    .join(&format!("target-{}", self.rustup_name())),
            );
        }
        let mut cmd = match cfg.args.script {
            Some(ref script) => {
                let mut cmd = Command::new(script);
                cmd.env("RUSTUP_TOOLCHAIN", self.rustup_name());
                cmd
            }
            None => {
                let mut cmd = Command::new("cargo");
                cmd.arg(&format!("+{}", self.rustup_name()));
                if cfg.args.cargo_args.is_empty() {
                    cmd.arg("build");
                } else {
                    cmd.args(&cfg.args.cargo_args);
                }
                cmd
            }
        };
        cmd.current_dir(&cfg.args.test_dir);
        cmd.env("CARGO_TARGET_DIR", format!("target-{}", self.rustup_name()));

        // let `cmd` capture stderr for us to process afterward.
        let must_capture_output = cfg.output_processing_mode().must_process_stderr();
        let emit_output = cfg.args.emit_cargo_output() || cfg.args.prompt;

        let default_stdio = || if must_capture_output {
            Stdio::piped()
        } else if emit_output {
            Stdio::inherit()
        } else {
            Stdio::null()
        };

        cmd.stdout(default_stdio());
        cmd.stderr(default_stdio());

        let output = match cmd.output() {
            Ok(output) => output,
            Err(err) => {
                panic!("failed to run {:?}: {:?}", cmd, err);
            }
        };

        // if we captured the stdout above but still need to emit it, then do so now
        if must_capture_output && emit_output {
            io::stdout().write_all(&output.stdout).unwrap();
            io::stderr().write_all(&output.stderr).unwrap();
        }
        output
    }

    fn install(&self, client: &Client, dl_params: &DownloadParams) -> Result<(), InstallError> {
        if self.is_current_nightly() {
            // pre existing installation
            return Ok(());
        }

        debug!("installing {}", self);
        let tmpdir = TempDir::new_in(&dl_params.tmp_dir, &self.rustup_name())
            .map_err(InstallError::TempDir)?;
        let dest = dl_params.install_dir.join(self.rustup_name());
        if dl_params.force_install {
            let _ = fs::remove_dir_all(&dest);
        }

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

        if dl_params.install_src {
            let filename = "rust-src-nightly";
            download_tarball(
                &client,
                "rust-src",
                &format!("{}/{}/{}.tar", dl_params.url_prefix, location, filename,),
                Some(&PathBuf::from(&filename).join("rust-src")),
                tmpdir.path(),
            ).map_err(InstallError::Download)?;
        }

        fs::rename(tmpdir.into_path(), dest).map_err(InstallError::Move)?;

        Ok(())
    }
}

struct Config {
    args: Opts,
    rustup_tmp_path: PathBuf,
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
        let mut args = args;

        let mut toolchains_path = match env::var_os("RUSTUP_HOME") {
            Some(h) => PathBuf::from(h),
            None => {
                let mut home = dirs::home_dir().ok_or_else(|| format_err!("Could not find home."))?;
                home.push(".rustup");
                home
            }
        };

        // We will download and extract the tarballs into this directory before installing.
        // Using `~/.rustup/tmp` instead of $TMPDIR ensures we could always perform installation by
        // renaming instead of copying the whole directory.
        let rustup_tmp_path = toolchains_path.join("tmp");
        if !rustup_tmp_path.exists() {
            fs::create_dir(&rustup_tmp_path)?;
        }

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
            eprintln!("finding commit range that corresponds to dates specified");
            match (args.start, args.end) {
                (Some(b1), Some(b2)) => {
                    args.start = Some(b1.as_commit()?);
                    args.end = Some(b2.as_commit()?);
                }
                _ => unreachable!(),
            }
        }

        Ok(Config {
            is_commit: args.by_commit || is_commit == Some(true),
            args,
            target,
            toolchains_path,
            rustup_tmp_path,
        })
    }
}

fn check_bounds(start: &Option<Bound>, end: &Option<Bound>) -> Result<(), Error> {
    match (&start, &end) {
        (Some(Bound::Date(start)), Some(Bound::Date(end))) if end < start => {
            bail!(
                "end should be after start, got start: {:?} and end {:?}",
		start,
		end
            );
	},
        _ => {}
    }

    Ok(())
}

fn run() -> Result<(), Error> {
    env_logger::try_init()?;
    let args = env::args_os().filter(|a| a != "bisect-rustc");
    let args = Opts::from_iter(args);
    check_bounds(&args.start, &args.end)?;
    let cfg = Config::from_args(args)?;

    let client = Client::new();

    if let Some(ref bound) = cfg.args.install {
        install(&cfg, &client, bound)
    } else {
        bisect(&cfg, &client)
    }
}

fn install(cfg: &Config, client: &Client, bound: &Bound) -> Result<(), Error> {
    match *bound {
        Bound::Commit(ref sha) => {
            let sha = git::expand_commit(sha)?;
            let mut t = Toolchain {
                spec: ToolchainSpec::Ci {
                    commit: sha.clone(),
                    alt: cfg.args.alt,
                },
                host: cfg.args.host.clone(),
                std_targets: vec![cfg.args.host.clone(), cfg.target.clone()],
            };
            t.std_targets.sort();
            t.std_targets.dedup();
            let dl_params = DownloadParams::for_ci(cfg);
            t.install(client, &dl_params)?;
        }
        Bound::Date(date) => {
            let mut t = Toolchain {
                spec: ToolchainSpec::Nightly { date: date },
                host: cfg.args.host.clone(),
                std_targets: vec![cfg.args.host.clone(), cfg.target.clone()],
            };
            t.std_targets.sort();
            t.std_targets.dedup();
            let dl_params = DownloadParams::for_nightly(cfg);
            t.install(client, &dl_params)?;
        }
    }

    Ok(())
}

fn bisect(cfg: &Config, client: &Client) -> Result<(), Error> {
    if cfg.is_commit {
        let bisection_result = bisect_ci(&cfg, &client)?;
        print_results(cfg, client, &bisection_result);
    } else {
        let nightly_bisection_result = bisect_nightlies(&cfg, &client)?;
        print_results(cfg, client, &nightly_bisection_result);
        let nightly_regression = &nightly_bisection_result.searched[nightly_bisection_result.found];

        if let ToolchainSpec::Nightly { date } = nightly_regression.spec {
            let previous_date = date - chrono::Duration::days(1);

            if let Bound::Commit(bad_commit) = Bound::Date(date).as_commit()? {
                if let Bound::Commit(working_commit) = Bound::Date(previous_date).as_commit()? {
                    eprintln!(
                        "looking for regression commit between {} and {}",
                        date.format("%Y-%m-%d"),
                        previous_date.format("%Y-%m-%d"),
                    );

                    let ci_bisection_result = bisect_ci_between(cfg, client, &working_commit, &bad_commit)?;
                    print_results(cfg, client, &ci_bisection_result);
                    print_final_report(&nightly_bisection_result, &ci_bisection_result);
                }
            }
        }
    }

    Ok(())
}

fn print_results(cfg: &Config, client: &Client, bisection_result: &BisectionResult) {
    let BisectionResult {
        searched: toolchains,
        dl_spec,
        found,
    } = bisection_result;

    eprintln!(
        "searched toolchains {} through {}",
        toolchains.first().unwrap(),
        toolchains.last().unwrap(),
    );

    if toolchains[*found] == *toolchains.last().unwrap() {
        let t = &toolchains[*found];
        let r = match t.install(&client, &dl_spec) {
            Ok(()) => {
                let outcome = t.test(&cfg);
                if !cfg.args.preserve {
                    let _ = t.remove(&dl_spec);
                }
                // we want to fail, so a successful build doesn't satisfy us
                match outcome {
                    TestOutcome::Baseline => Satisfies::No,
                    TestOutcome::Regressed => Satisfies::Yes,
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
                return;
            }
        }
    }

    eprintln!("regression in {}", toolchains[*found]);
}

fn print_final_report(
    nightly_bisection_result: &BisectionResult,
    ci_bisection_result: &BisectionResult,
) {
    let BisectionResult {
        searched: nightly_toolchains,
        found: nightly_found,
        ..
    } = nightly_bisection_result;

    let BisectionResult {
        searched: ci_toolchains,
        found: ci_found,
        ..
    } = ci_bisection_result;

    eprintln!("");
    eprintln!("");

    eprintln!("==================================================================================");
    eprintln!("= Please open an issue on Rust's github repository                               =");
    eprintln!("= https://github.com/rust-lang/rust/issues/new                                   =");
    eprintln!("= Below you will find a text that would serve as a starting point of your report =");
    eprintln!("==================================================================================");

    eprintln!("");

    eprintln!("# Regression found in the compiler");
    eprintln!("");

    eprintln!(
        "searched nightlies: from {} to {}",
        nightly_toolchains.first().unwrap(),
        nightly_toolchains.last().unwrap(),
    );

    eprintln!(
        "regressed nightly: {}",
        nightly_toolchains[*nightly_found],
    );

    eprintln!(
        "searched commits: from https://github.com/rust-lang/rust/commit/{} to https://github.com/rust-lang/rust/commit/{1}",
        ci_toolchains.first().unwrap(),
        ci_toolchains.last().unwrap(),
    );

    eprintln!(
        "regressed commit: https://github.com/rust-lang/rust/commit/{}",
        ci_toolchains[*ci_found],
    );

    eprintln!("source code: URL OF A REPOSITORY THAT REPRODUCES THE ERROR");

    eprintln!("");

    eprintln!("## Instructions");
    eprintln!("");
    eprintln!("Please give the steps for how to build your repository (platform, system dependencies, etc.)");

    eprintln!("## Error");
    eprintln!("");
    eprintln!("<details><summary>COLLAPSIBLE ERROR STACKTRACE</summary>");
    eprintln!("<p>");
    eprintln!("");
    eprintln!("```bash");
    eprintln!("Paste the error the compiler is giving");
    eprintln!("```");
    eprintln!("");
    eprintln!("</p></details>");
}

struct NightlyFinderIter {
    start_date: Date<Utc>,
    current_date: Date<Utc>,
}

impl NightlyFinderIter {
    fn new(start_date: Date<Utc>) -> Self {
        Self {
            start_date,
            current_date: start_date,
        }
    }
}

impl Iterator for NightlyFinderIter {
    type Item = Date<Utc>;

    fn next(&mut self) -> Option<Date<Utc>> {
        let current_distance = self.start_date - self.current_date;

        let jump_length =
            if current_distance.num_days() < 7 {
                // first week jump by two days
                2
            } else if current_distance.num_days() < 49 {
                // from 2nd to 7th week jump weekly
                7
            } else {
                // from 7th week jump by two weeks
                14
            };

        self.current_date = self.current_date - chrono::Duration::days(jump_length);
        Some(self.current_date)
    }
}

#[test]
fn test_nightly_finder_iterator() {
    let start_date = chrono::Date::from_utc(
        chrono::naive::NaiveDate::from_ymd(2019, 01, 01),
        chrono::Utc,
    );

    let mut iter = NightlyFinderIter::new(start_date);

    assert_eq!(start_date - chrono::Duration::days(2), iter.next().unwrap());
    assert_eq!(start_date - chrono::Duration::days(4), iter.next().unwrap());
    assert_eq!(start_date - chrono::Duration::days(6), iter.next().unwrap());
    assert_eq!(start_date - chrono::Duration::days(8), iter.next().unwrap());
    assert_eq!(start_date - chrono::Duration::days(15), iter.next().unwrap());
    assert_eq!(start_date - chrono::Duration::days(22), iter.next().unwrap());
    assert_eq!(start_date - chrono::Duration::days(29), iter.next().unwrap());
    assert_eq!(start_date - chrono::Duration::days(36), iter.next().unwrap());
    assert_eq!(start_date - chrono::Duration::days(43), iter.next().unwrap());
    assert_eq!(start_date - chrono::Duration::days(50), iter.next().unwrap());
    assert_eq!(start_date - chrono::Duration::days(64), iter.next().unwrap());
    assert_eq!(start_date - chrono::Duration::days(78), iter.next().unwrap());
}

fn bisect_nightlies(cfg: &Config, client: &Client) -> Result<BisectionResult, Error> {
    if cfg.args.alt {
        bail!("cannot bisect nightlies with --alt: not supported");
    }

    let dl_spec = DownloadParams::for_nightly(&cfg);

    // before this date we didn't have -std packages
    let end_at = chrono::Date::from_utc(
        chrono::naive::NaiveDate::from_ymd(2015, 10, 20),
        chrono::Utc,
    );
    let mut first_success = None;

    let mut last_failure = if let Some(Bound::Date(date)) = cfg.args.end {
        date
    } else {
        if let Some(date) = Toolchain::default_nightly() {
            date
        } else {
            chrono::Utc::now().date()
        }
    };

    let (mut nightly_date, has_start) = if let Some(Bound::Date(date)) = cfg.args.start {
        (date, true)
    } else {
        (last_failure, false)
    };

    let mut nightly_iter = NightlyFinderIter::new(nightly_date);

    while nightly_date > end_at {
        let mut t = Toolchain {
            spec: ToolchainSpec::Nightly { date: nightly_date },
            host: cfg.args.host.clone(),
            std_targets: vec![cfg.args.host.clone(), cfg.target.clone()],
        };
        t.std_targets.sort();
        t.std_targets.dedup();
        if t.is_current_nightly() {
            eprintln!("checking {} from the currently installed default nightly \
                       toolchain as the last failure", t);
        } else {
            eprintln!("checking {}", t);
        }
        match t.install(client, &dl_spec) {
            Ok(()) => {
                let outcome = t.test(&cfg);

                if !cfg.args.preserve {
                    let _ = t.remove(&dl_spec);
                }

                if let TestOutcome::Baseline = outcome {
                    first_success = Some(nightly_date);
                    break;
                } else if has_start {
                    return Err(format_err!("the --start nightly has the regression"))?;
                } else {
                    last_failure = nightly_date;
                }

                nightly_date = nightly_iter.next().unwrap();
            }
            Err(InstallError::NotFound { .. }) => {
                // go back just one day, presumably missing nightly
                nightly_date = nightly_date - chrono::Duration::days(1);
                if !cfg.args.preserve {
                    let _ = t.remove(&dl_spec);
                }
                if has_start {
                    return Err(format_err!("could not find the --start nightly"))?;
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
                let outcome = t.test(&cfg);
                // we want to fail, so a successful build doesn't satisfy us
                let r = match outcome {
                    TestOutcome::Baseline => Satisfies::No,
                    TestOutcome::Regressed => Satisfies::Yes,
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
    })?;

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
    eprintln!("bisecting ci builds");
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

    eprintln!("starting at {}, ending at {}", start, end);

    bisect_ci_between(cfg, client, start, end)
}

fn bisect_ci_between(cfg: &Config, client: &Client, start: &str, end: &str) -> Result<BisectionResult, Error> {
    let dl_spec = DownloadParams::for_ci(cfg);
    let mut commits = get_commits(start, end)?;
    let now = chrono::Utc::now();
    commits.retain(|c| now.signed_duration_since(c.date).num_days() < 167);

    if commits.is_empty() {
        bail!(
            "no commits between {} and {} within last 167 days",
            start,
            end
        );
    }

    if let Some(ref c) = commits.last() {
        if end != "origin/master" && !c.sha.starts_with(end) {
            bail!("expected to end with {}, but ended with {}", end, c.sha);
        }
    }

    eprintln!("validated commits found, specifying toolchains");

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

    eprintln!("testing commits");
    let found = least_satisfying(&toolchains, |t| {
        eprintln!("installing {}", t);
        match t.install(&client, &dl_spec) {
            Ok(()) => {
                eprintln!("testing {}", t);
                let outcome = t.test(&cfg);
                // we want to fail, so a successful build doesn't satisfy us
                let r = match outcome {
                    TestOutcome::Regressed => Satisfies::Yes,
                    TestOutcome::Baseline => Satisfies::No,
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
    })?;

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
        match err.downcast::<ExitStatusError>() {
            Ok(ExitStatusError(code)) => process::exit(code),
            Err(err) => {
                eprintln!("Error: {}", err);
                process::exit(1);
            }
        }
    }
}

use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::iter;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};

use chrono::{Date, naive, Utc};
use colored::*;
use dialoguer::Select;
use failure::{Fail, Error};
use flate2::read::GzDecoder;
use log::debug;
use once_cell::sync::OnceCell;
use pbr::{ProgressBar, Units};
use regex::Regex;
use reqwest::blocking::{Client, Response};
use reqwest::header::CONTENT_LENGTH;
use rustc_version::Channel;
use tar::Archive;
use tee::TeeReader;
use tempdir::TempDir;
use xz2::read::XzDecoder;

use crate::{Config, CommandTemplate};

pub type GitDate = Date<Utc>;

const YYYY_MM_DD: &str = "%Y-%m-%d";

pub(crate) const NIGHTLY_SERVER: &str = "https://static.rust-lang.org/dist";
const CI_SERVER: &str = "https://s3-us-west-1.amazonaws.com/rust-lang-ci2";

#[derive(Fail, Debug)]
pub(crate) enum InstallError {
    #[fail(display = "Could not find {}; url: {}", spec, url)]
    NotFound { url: String, spec: ToolchainSpec },
    #[fail(display = "Could not download toolchain: {}", _0)]
    Download(#[cause] DownloadError),
    #[fail(display = "Could not create tempdir: {}", _0)]
    TempDir(#[cause] io::Error),
    #[fail(display = "Could not move tempdir into destination: {}", _0)]
    Move(#[cause] io::Error),
    #[fail(display = "Could not run subcommand {}: {}", command, cause)]
    Subcommand {
        command: String,
        #[cause]
        cause: io::Error,
    },
}

#[derive(Debug)]
pub(crate) enum TestOutcome {
    Baseline,
    Regressed,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Toolchain {
    pub(crate) spec: ToolchainSpec,
    pub(crate) host: String,
    pub(crate) std_targets: Vec<String>,
}

impl fmt::Display for Toolchain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.spec)
    }
}

impl Toolchain {
    pub(crate) fn rustup_name(&self) -> String {
        match self.spec {
            ToolchainSpec::Ci { ref commit, alt } => {
                let alt_s = if alt {
                    "-alt".to_string()
                } else {
                    String::new()
                };
                format!("bisector-ci-{}{}-{}", commit, alt_s, self.host)
            }
            // N.B. We need to call this with a nonstandard name so that rustup utilizes the
            // fallback cargo logic.
            ToolchainSpec::Nightly { ref date } => {
                format!("bisector-nightly-{}-{}", date.format(YYYY_MM_DD), self.host)
            }
        }
    }
    /// This returns the date of the default toolchain, if it is a nightly toolchain.
    /// Returns `None` if the installed toolchain is not a nightly toolchain.
    pub(crate) fn default_nightly() -> Option<GitDate> {
        fn inner() -> Option<GitDate> {
            let version_meta = rustc_version::version_meta().unwrap();

            if let Channel::Nightly = version_meta.channel {
                if let Some(str_date) = version_meta.commit_date {
                    let regex = Regex::new(r"(?m)^(\d{4})-(\d{2})-(\d{2})$").unwrap();
                    if let Some(cap) = regex.captures(&str_date) {
                        let year = cap.get(1)?.as_str().parse::<i32>().ok()?;
                        let month = cap.get(2)?.as_str().parse::<u32>().ok()?;
                        let day = cap.get(3)?.as_str().parse::<u32>().ok()?;

                        // rustc commit date is off-by-one.
                        let date = naive::NaiveDate::from_ymd(year, month, day).succ();

                        return Some(Date::from_utc(date, Utc));
                    }
                }
            }

            None
        }

        static DATE: OnceCell<Option<GitDate>> = OnceCell::new();
        *(DATE.get_or_init(inner))
    }

    pub(crate) fn is_current_nightly(&self) -> bool {
        if let ToolchainSpec::Nightly { date } = self.spec {
            if let Some(default_date) = Self::default_nightly() {
                return default_date == date;
            }
        }

        false
    }

    pub(crate) fn install(
        &self,
        client: &Client,
        dl_params: &DownloadParams,
    ) -> Result<(), InstallError> {
        let tc_stdstream_str = format!("{}", self);
        eprintln!("installing {}", tc_stdstream_str.green());
        let tmpdir = TempDir::new_in(&dl_params.tmp_dir, &self.rustup_name())
            .map_err(InstallError::TempDir)?;
        let dest = dl_params.install_dir.join(self.rustup_name());
        if dl_params.force_install {
            let _ = self.do_remove(dl_params);
        }

        if dest.is_dir() {
            // already installed
            return Ok(());
        }

        if self.is_current_nightly() {
            // make link to pre-existing installation
            debug!("installing (via link) {}", self);

            let nightly_path: String = {
                let cmd = CommandTemplate::new(
                    ["rustc", "--print", "sysroot"]
                        .iter()
                        .map(|s| (*s).to_string()),
                );
                let stdout = cmd.output()?.stdout;
                let output = String::from_utf8_lossy(&stdout);
                // the output should be the path, terminated by a newline
                let mut path = output.to_string();
                let last = path.pop();
                assert_eq!(last, Some('\n'));
                path
            };

            let cmd = CommandTemplate::new(
                ["rustup", "toolchain", "link"]
                    .iter()
                    .map(|s| (*s).to_string())
                    .chain(iter::once(self.rustup_name()))
                    .chain(iter::once(nightly_path)),
            );
            if cmd.status()?.success() {
                return Ok(());
            } else {
                return Err(InstallError::Subcommand {
                    command: cmd.string(),
                    cause: io::Error::new(io::ErrorKind::Other, "failed to link via `rustup`"),
                });
            }
        }

        debug!("installing via download {}", self);

        let rustc_filename = format!("rustc-nightly-{}", self.host);

        let location = match self.spec {
            ToolchainSpec::Ci { ref commit, .. } => commit.to_string(),
            ToolchainSpec::Nightly { ref date } => date.format(YYYY_MM_DD).to_string(),
        };

        // download rustc.
        if let Err(e) = download_tarball(
            client,
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
                        url,
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
                client,
                &format!("std for {}", target),
                &format!(
                    "{}/{}/{}.tar",
                    dl_params.url_prefix, location, rust_std_filename
                ),
                Some(
                    &PathBuf::from(&rust_std_filename)
                        .join(format!("rust-std-{}", target))
                        .join("lib"),
                ),
                &tmpdir.path().join("lib"),
            )
            .map_err(InstallError::Download)?;
        }

        // download cargo by default
        // deactivate with the `--without-cargo` flag
        // this default behavior was changed as of v0.6.0
        // see: https://github.com/rust-lang/cargo-bisect-rustc/issues/81
        if !dl_params.without_cargo {
            let filename = format!("cargo-nightly-{}", self.host);
            download_tarball(
                client,
                &format!("cargo for {}", self.host),
                &format!("{}/{}/{}.tar", dl_params.url_prefix, location, filename,),
                Some(&PathBuf::from(&filename).join("cargo")),
                tmpdir.path(),
            )
            .map_err(InstallError::Download)?;
        }

        if dl_params.install_src {
            let filename = "rust-src-nightly";
            download_tarball(
                client,
                "rust-src",
                &format!("{}/{}/{}.tar", dl_params.url_prefix, location, filename,),
                Some(&PathBuf::from(&filename).join("rust-src")),
                tmpdir.path(),
            )
            .map_err(InstallError::Download)?;
        }

        if dl_params.install_dev {
            let filename = format!("rustc-dev-nightly-{}", self.host);
            download_tarball(
                client,
                "rustc-dev",
                &format!("{}/{}/{}.tar", dl_params.url_prefix, location, filename,),
                Some(&PathBuf::from(&filename).join(format!("rustc-dev-{}", self.host))),
                tmpdir.path(),
            )
            .map_err(InstallError::Download)?;
            // llvm-tools-(preview) is currently required for using rustc-dev https://github.com/rust-lang/rust/issues/72594
            let filename = format!("llvm-tools-nightly-{}", self.host);
            download_tarball(
                client,
                "llvm-tools",
                &format!("{}/{}/{}.tar", dl_params.url_prefix, location, filename,),
                Some(&PathBuf::from(&filename).join("llvm-tools-preview")),
                tmpdir.path(),
            )
            .map_err(InstallError::Download)?;
        }

        fs::rename(tmpdir.into_path(), dest).map_err(InstallError::Move)?;

        Ok(())
    }

    pub(crate) fn remove(&self, dl_params: &DownloadParams) -> Result<(), Error> {
        eprintln!("uninstalling {}", self);
        self.do_remove(dl_params)
    }

    /// Removes the (previously installed) bisector rustc described by `dl_params`.
    ///
    /// The main reason to call this (instead of `fs::remove_dir_all` directly)
    /// is to guard against deleting state not managed by `cargo-bisect-rustc`.
    pub(crate) fn do_remove(&self, dl_params: &DownloadParams) -> Result<(), Error> {
        let rustup_name = self.rustup_name();

        // Guard against destroying directories that this tool didn't create.
        assert!(
            rustup_name.starts_with("bisector-nightly") || rustup_name.starts_with("bisector-ci")
        );

        let dir = dl_params.install_dir.join(rustup_name);
        fs::remove_dir_all(&dir)?;

        Ok(())
    }

    pub(crate) fn run_test(&self, cfg: &Config) -> process::Output {
        if !cfg.args.preserve_target {
            let _ = fs::remove_dir_all(
                cfg.args
                    .test_dir
                    .join(&format!("target-{}", self.rustup_name())),
            );
        }

        let mut cmd = match (cfg.args.script.as_ref(), cfg.args.timeout) {
            (Some(script), None) => {
                let mut cmd = Command::new(script);
                cmd.env("RUSTUP_TOOLCHAIN", self.rustup_name());
                cmd.args(&cfg.args.command_args);
                cmd
            }
            (None, None) => {
                let mut cmd = Command::new("cargo");
                cmd.arg(&format!("+{}", self.rustup_name()));
                if cfg.args.command_args.is_empty() {
                    cmd.arg("build");
                } else {
                    cmd.args(&cfg.args.command_args);
                }
                cmd
            }
            (Some(script), Some(timeout)) => {
                let mut cmd = Command::new("timeout");
                cmd.arg(timeout.to_string());
                cmd.arg(script);
                cmd.args(&cfg.args.command_args);
                cmd.env("RUSTUP_TOOLCHAIN", self.rustup_name());
                cmd
            }
            (None, Some(timeout)) => {
                let mut cmd = Command::new("timeout");
                cmd.arg(timeout.to_string());
                cmd.arg("cargo");
                cmd.arg(format!("+{}", self.rustup_name()));
                if cfg.args.command_args.is_empty() {
                    cmd.arg("build");
                } else {
                    cmd.args(&cfg.args.command_args);
                }
                cmd
            }
        };
        cmd.current_dir(&cfg.args.test_dir);
        cmd.env("CARGO_TARGET_DIR", format!("target-{}", self.rustup_name()));

        // let `cmd` capture stderr for us to process afterward.
        let must_capture_output = cfg.regress_on().must_process_stderr();
        let emit_output = cfg.args.emit_cargo_output() || cfg.args.prompt;

        let default_stdio = || {
            if must_capture_output {
                Stdio::piped()
            } else if emit_output {
                Stdio::inherit()
            } else {
                Stdio::null()
            }
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

    pub(crate) fn test(&self, cfg: &Config) -> TestOutcome {
        eprintln!("testing...");
        let outcome = if cfg.args.prompt {
            loop {
                let output = self.run_test(cfg);
                let status = output.status;

                //timeout returns exit code 124 on expiration
                if status.code() == Some(124) {
                    match cfg.args.timeout {
                        Some(_) => break TestOutcome::Regressed,
                        None => panic!("Process timed out but no timeout was specified. Please check host configuration for timeouts and try again.")
                    }
                }

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

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum ToolchainSpec {
    Ci { commit: String, alt: bool },
    Nightly { date: GitDate },
}

impl fmt::Display for ToolchainSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ToolchainSpec::Ci { ref commit, alt } => {
                let alt_s = if alt {
                    "-alt".to_string()
                } else {
                    String::new()
                };
                write!(f, "{}{}", commit, alt_s)
            }
            ToolchainSpec::Nightly { ref date } => write!(f, "nightly-{}", date.format(YYYY_MM_DD)),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DownloadParams {
    url_prefix: String,
    tmp_dir: PathBuf,
    install_dir: PathBuf,
    install_src: bool,
    install_dev: bool,
    without_cargo: bool,
    force_install: bool,
}

impl DownloadParams {
    pub(crate) fn for_ci(cfg: &Config) -> Self {
        let url_prefix = format!(
            "{}/rustc-builds{}",
            CI_SERVER,
            if cfg.args.alt { "-alt" } else { "" }
        );

        Self::from_cfg_with_url_prefix(cfg, url_prefix)
    }

    pub(crate) fn for_nightly(cfg: &Config) -> Self {
        Self::from_cfg_with_url_prefix(cfg, NIGHTLY_SERVER.to_string())
    }

    pub(crate) fn from_cfg_with_url_prefix(cfg: &Config, url_prefix: String) -> Self {
        DownloadParams {
            url_prefix,
            tmp_dir: cfg.rustup_tmp_path.clone(),
            install_dir: cfg.toolchains_path.clone(),
            install_src: cfg.args.with_src,
            install_dev: cfg.args.with_dev,
            without_cargo: cfg.args.without_cargo,
            force_install: cfg.args.force_install,
        }
    }
}

#[derive(Fail, Debug)]
pub(crate) enum ArchiveError {
    #[fail(display = "Failed to parse archive: {}", _0)]
    Archive(#[cause] io::Error),
    #[fail(display = "Failed to create directory: {}", _0)]
    CreateDir(#[cause] io::Error),
}

#[derive(Fail, Debug)]
pub(crate) enum DownloadError {
    #[fail(display = "Tarball not found at {}", _0)]
    NotFound(String),
    #[fail(display = "A reqwest error occurred: {}", _0)]
    Reqwest(#[cause] reqwest::Error),
    #[fail(display = "An archive error occurred: {}", _0)]
    Archive(#[cause] ArchiveError),
}

pub(crate) fn download_progress(
    client: &Client,
    name: &str,
    url: &str,
) -> Result<(Response, ProgressBar<io::Stdout>), DownloadError> {
    debug!("downloading <{}>...", url);

    let response = client.get(url).send().map_err(DownloadError::Reqwest)?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(DownloadError::NotFound(url.to_string()));
    }
    let response = response
        .error_for_status()
        .map_err(DownloadError::Reqwest)?;

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

pub(crate) fn download_tar_xz(
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

pub(crate) fn download_tar_gz(
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

pub(crate) fn unarchive<R: Read>(
    r: R,
    strip_prefix: Option<&Path>,
    dest: &Path,
) -> Result<(), ArchiveError> {
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

pub(crate) fn download_tarball(
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

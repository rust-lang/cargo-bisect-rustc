use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};

use chrono::{Date, NaiveDate, Utc};
use colored::Colorize;
use dialoguer::Select;
use anyhow::Error;
use flate2::read::GzDecoder;
use log::debug;
use pbr::{ProgressBar, Units};
use reqwest::blocking::{Client, Response};
use reqwest::header::CONTENT_LENGTH;
use rustc_version::Channel;
use tar::Archive;
use tee::TeeReader;
use tempdir::TempDir;
use xz2::read::XzDecoder;

use crate::Config;

pub type GitDate = Date<Utc>;

pub const YYYY_MM_DD: &str = "%Y-%m-%d";

pub(crate) const NIGHTLY_SERVER: &str = "https://static.rust-lang.org/dist";
const CI_SERVER: &str = "https://s3-us-west-1.amazonaws.com/rust-lang-ci2";

#[derive(thiserror::Error, Debug)]
pub(crate) enum InstallError {
    #[error("Could not find {spec}; url: {url}")]
    NotFound { url: String, spec: ToolchainSpec },
    #[error("Could not download toolchain: {0}")]
    Download(#[source] DownloadError),
    #[error("Could not create tempdir: {0}")]
    TempDir(#[source] io::Error),
    #[error("Could not move tempdir into destination: {0}")]
    Move(#[source] io::Error),
    #[error("Could not run subcommand {cmd}: {err}")]
    Subcommand {
        cmd: String,
        #[source]
        err: io::Error,
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
        rustc_version::version_meta()
            .ok()
            .filter(|v| v.channel == Channel::Nightly)
            // rustc commit date is off-by-one, see #112
            .and_then(|v| parse_to_utc_date(&v.commit_date?).ok().map(|d| d.succ()))
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
                let mut cmd = Command::new("rustc");
                cmd.args(["--print", "sysroot"]);

                let stdout = cmd
                    .output()
                    .map_err(|err| InstallError::Subcommand {
                        cmd: format!("{cmd:?}"),
                        err,
                    })?
                    .stdout;
                let output = String::from_utf8_lossy(&stdout);
                // the output should be the path, terminated by a newline
                let mut path = output.to_string();
                let last = path.pop();
                assert_eq!(last, Some('\n'));
                path
            };
            let mut cmd = Command::new("rustup");
            cmd.args(["toolchain", "link", &self.rustup_name(), &nightly_path]);
            let status = cmd.status().map_err(|err| InstallError::Subcommand {
                cmd: format!("{cmd:?}"),
                err,
            })?;
            return if status.success() {
                Ok(())
            } else {
                Err(InstallError::Subcommand {
                    cmd: format!("{cmd:?}"),
                    err: io::Error::new(
                        io::ErrorKind::Other,
                        "thiserror::Errored to link via `rustup`",
                    ),
                })
            };
        }

        debug!("installing via download {}", self);

        let location = match self.spec {
            ToolchainSpec::Ci { ref commit, .. } => commit.to_string(),
            ToolchainSpec::Nightly { ref date } => date.format(YYYY_MM_DD).to_string(),
        };

        let components = dl_params
            .components
            .iter()
            .map(|component| {
                if component == "rust-src" {
                    // rust-src is target-independent
                    "rust-src-nightly".to_string()
                } else {
                    format!("{}-nightly-{}", component, self.host)
                }
            })
            .chain(
                self.std_targets
                    .iter()
                    .map(|target| format!("rust-std-nightly-{}", target)),
            );

        for component in components {
            download_tarball(
                client,
                &component,
                &format!("{}/{}/{}.tar", dl_params.url_prefix, location, component),
                tmpdir.path(),
            )
            .map_err(|e| {
                if let DownloadError::NotFound(url) = e {
                    InstallError::NotFound {
                        url,
                        spec: self.spec.clone(),
                    }
                } else {
                    InstallError::Download(e)
                }
            })?;
        }

        fs::rename(tmpdir.into_path(), dest).map_err(InstallError::Move)
    }

    pub(crate) fn remove(&self, dl_params: &DownloadParams) -> Result<(), Error> {
        eprintln!("uninstalling {}", self);
        self.do_remove(dl_params)
    }

    /// Removes the (previously installed) bisector rustc described by `dl_params`.
    ///
    /// The main reason to call this (instead of `fs::remove_dir_all` directly)
    /// is to guard against deleting state not managed by `cargo-bisect-rustc`.
    fn do_remove(&self, dl_params: &DownloadParams) -> Result<(), Error> {
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

        let default_stdio = if must_capture_output {
            Stdio::piped
        } else if emit_output {
            Stdio::inherit
        } else {
            Stdio::null
        };

        cmd.stdout(default_stdio());
        cmd.stderr(default_stdio());

        let output = match cmd.output() {
            Ok(output) => output,
            Err(err) => {
                panic!("thiserror::Errored to run {:?}: {:?}", cmd, err);
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

                let default_choice = match cfg.default_outcome_of_output(&output) {
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
            cfg.default_outcome_of_output(&output)
        };

        outcome
    }
}

pub fn parse_to_utc_date(s: &str) -> chrono::ParseResult<GitDate> {
    NaiveDate::parse_from_str(s, YYYY_MM_DD).map(|date| Date::from_utc(date, Utc))
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
    components: Vec<String>,
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

    fn from_cfg_with_url_prefix(cfg: &Config, url_prefix: String) -> Self {
        let mut components = vec!["rustc".to_string()];
        if !cfg.args.without_cargo {
            components.push("cargo".to_string());
        }
        if cfg.args.with_dev {
            components.push("rustc-dev".to_string());
            // llvm-tools-(preview) is currently required for using rustc-dev
            // https://github.com/rust-lang/rust/issues/72594
            components.push("llvm-tools".to_string());
        }
        if cfg.args.with_src {
            components.push("rust-src".to_string());
        }
        components.extend(cfg.args.components.clone());

        DownloadParams {
            url_prefix,
            tmp_dir: cfg.rustup_tmp_path.clone(),
            install_dir: cfg.toolchains_path.clone(),
            components,
            force_install: cfg.args.force_install,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ArchiveError {
    #[error("thiserror::Errored to parse archive: {0}")]
    Archive(#[source] io::Error),
    #[error("thiserror::Errored to create directory: {0}")]
    CreateDir(#[source] io::Error),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum DownloadError {
    #[error("Tarball not found at {0}")]
    NotFound(String),
    #[error("A reqwest error occurred: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("An archive error occurred: {0}")]
    Archive(#[from] ArchiveError),
}

pub(crate) fn download_progress(
    client: &Client,
    name: &str,
    url: &str,
) -> Result<TeeReader<Response, ProgressBar<io::Stdout>>, DownloadError> {
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
        .and_then(|c| c.to_str().ok()?.parse().ok())
        .unwrap_or(0);
    let mut bar = ProgressBar::new(length);
    bar.set_units(Units::Bytes);
    bar.message(&format!("{name}: "));

    Ok(TeeReader::new(response, bar))
}

fn download_tar_xz(
    client: &Client,
    name: &str,
    url: &str,
    dest: &Path,
) -> Result<(), DownloadError> {
    let response = XzDecoder::new(download_progress(client, name, url)?);
    unarchive(response, dest).map_err(DownloadError::Archive)
}

fn download_tar_gz(
    client: &Client,
    name: &str,
    url: &str,
    dest: &Path,
) -> Result<(), DownloadError> {
    let response = GzDecoder::new(download_progress(client, name, url)?);
    unarchive(response, dest).map_err(DownloadError::Archive)
}

fn unarchive<R: Read>(r: R, dest: &Path) -> Result<(), ArchiveError> {
    for entry in Archive::new(r).entries().map_err(ArchiveError::Archive)? {
        let mut entry = entry.map_err(ArchiveError::Archive)?;
        let entry_path = entry.path().map_err(ArchiveError::Archive)?;
        let dest_path = {
            let mut components = entry_path.components();
            // Remove the first two components, which are usually of the form
            // COMPONENT-nightly-HOST/COMPONENT.
            components.next();
            // The second component here may also include some top-level
            // things like license files and install scripts. These will be
            // skipped in the check below if the path is empty.
            components.next();
            dest.join(components.as_path())
        };
        if dest_path == dest {
            // Skip root dir and files outside of "COMPONENT".
            continue;
        }
        fs::create_dir_all(dest_path.parent().unwrap()).map_err(ArchiveError::CreateDir)?;
        entry.unpack(dest_path).map_err(ArchiveError::Archive)?;
    }

    Ok(())
}

fn download_tarball(
    client: &Client,
    name: &str,
    url: &str,
    dest: &Path,
) -> Result<(), DownloadError> {
    match download_tar_xz(client, name, &format!("{}.xz", url,), dest) {
        Err(DownloadError::NotFound { .. }) => {
            download_tar_gz(client, name, &format!("{}.gz", url,), dest)
        }
        res => res,
    }
}

#![warn(clippy::pedantic)]
#![warn(clippy::cargo)]
#![allow(clippy::semicolon_if_nothing_returned)]
#![allow(clippy::let_underscore_drop)]
#![allow(clippy::single_match_else)]

use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::process;

use anyhow::{bail, Context};
use chrono::{Duration, NaiveDate, Utc};
use clap::{ArgAction, Parser, ValueEnum};
use colored::Colorize;
use github::get_pr_comments;
use log::debug;
use regex::RegexBuilder;
use reqwest::blocking::Client;

mod bounds;
mod git;
mod github;
mod least_satisfying;
mod repo_access;
mod toolchains;

use crate::bounds::{Bound, Bounds};
use crate::github::get_commit;
use crate::least_satisfying::{least_satisfying, Satisfies};
use crate::repo_access::{AccessViaGithub, AccessViaLocalGit, RustRepositoryAccessor};
use crate::toolchains::{
    parse_to_naive_date, DownloadError, DownloadParams, InstallError, TestOutcome, Toolchain,
    ToolchainSpec, YYYY_MM_DD,
};

const BORS_AUTHOR: &str = "bors";

#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    pub sha: String,
    pub date: GitDate,
    pub summary: String,
    pub committer: Author,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Author {
    pub name: String,
    pub email: String,
    pub date: GitDate,
}

/// The first commit which build artifacts are made available through the CI for
/// bisection.
///
/// Due to our deletion policy which expires builds after 167 days, the build
/// artifacts of this commit itself is no longer available, so this may not be entirely useful;
/// however, it does limit the amount of commits somewhat.
const EPOCH_COMMIT: &str = "927c55d86b0be44337f37cf5b0a76fb8ba86e06c";

const REPORT_HEADER: &str = "\
==================================================================================
= Please file this regression report on the rust-lang/rust GitHub repository     =
=        New issue: https://github.com/rust-lang/rust/issues/new                 =
=     Known issues: https://github.com/rust-lang/rust/issues                     =
= Copy and paste the text below into the issue report thread.  Thanks!           =
==================================================================================";

#[derive(Debug, Parser)]
#[command(bin_name = "cargo", subcommand_required = true)]
enum Cargo {
    BisectRustc(Opts),
}

#[derive(Debug, Parser)]
#[command(
    bin_name = "cargo bisect-rustc",
    version,
    about,
    next_display_order = None,
    after_help = "Examples:
    Run a fully automatic nightly bisect doing `cargo check`:
    ```
    cargo bisect-rustc --start 2018-07-07 --end 2018-07-30 --test-dir ../my_project/ -- check
    ```

    Run a PR-based bisect with manual prompts after each run doing `cargo build`:
    ```
    cargo bisect-rustc --start 6a1c0637ce44aeea6c60527f4c0e7fb33f2bcd0d \\
      --end 866a713258915e6cbb212d135f751a6a8c9e1c0a --test-dir ../my_project/ --prompt -- build
    ```"
)]
#[allow(clippy::struct_excessive_bools)]
struct Opts {
    #[arg(
        long,
        help = "Custom regression definition",
        value_enum,
        default_value_t = RegressOn::Error,
    )]
    regress: RegressOn,

    #[arg(short, long, help = "Download the alt build instead of normal build")]
    alt: bool,

    #[arg(
        long,
        help = "Host triple for the compiler",
        default_value = env!("HOST"),
    )]
    host: String,

    #[arg(long, help = "Cross-compilation target platform")]
    target: Option<String>,

    #[arg(long, help = "Preserve the downloaded artifacts")]
    preserve: bool,

    #[arg(long, help = "Preserve the target directory used for builds")]
    preserve_target: bool,

    #[arg(long, help = "Download rust-src [default: no download]")]
    with_src: bool,

    #[arg(long, help = "Download rustc-dev [default: no download]")]
    with_dev: bool,

    #[arg(short, long = "component", help = "additional components to install")]
    components: Vec<String>,

    #[arg(
        long,
        help = "Root directory for tests",
        default_value = ".",
        value_parser = validate_dir
    )]
    test_dir: PathBuf,

    #[arg(long, help = "Manually evaluate for regression with prompts")]
    prompt: bool,

    #[arg(
        long,
        short,
        help = "Assume failure after specified number of seconds (for bisecting hangs)"
    )]
    timeout: Option<usize>,

    #[arg(short, long = "verbose", action = ArgAction::Count)]
    verbosity: u8,

    #[arg(
        help = "Arguments to pass to cargo or the file specified by --script during tests",
        num_args = 1..,
        last = true
    )]
    command_args: Vec<OsString>,

    #[arg(
        long,
        help = "Left bound for search (*without* regression). You can use \
a date (YYYY-MM-DD), git tag name (e.g. 1.58.0) or git commit SHA."
    )]
    start: Option<Bound>,

    #[arg(
        long,
        help = "Right bound for search (*with* regression). You can use \
a date (YYYY-MM-DD), git tag name (e.g. 1.58.0) or git commit SHA."
    )]
    end: Option<Bound>,

    #[arg(long, help = "Bisect via commit artifacts")]
    by_commit: bool,

    #[arg(long, value_enum, help = "How to access Rust git repository", default_value_t = Access::Github)]
    access: Access,

    #[arg(long, help = "Install the given artifact")]
    install: Option<Bound>,

    #[arg(long, help = "Force installation over existing artifacts")]
    force_install: bool,

    #[arg(long, help = "Script replacement for `cargo build` command")]
    script: Option<PathBuf>,

    #[arg(long, help = "Do not install cargo [default: install cargo]")]
    without_cargo: bool,

    #[arg(
        long,
        help = "Text shown when a test does match the condition requested"
    )]
    term_new: Option<String>,

    #[arg(
        long,
        help = "Text shown when a test fails to match the condition requested"
    )]
    term_old: Option<String>,
}

pub type GitDate = NaiveDate;

pub fn today() -> NaiveDate {
    Utc::now().date_naive()
}

fn validate_dir(s: &str) -> anyhow::Result<PathBuf> {
    let path: PathBuf = s.parse()?;
    if path.is_dir() {
        Ok(path)
    } else {
        bail!(
            "{} is not an existing directory",
            path.canonicalize()?.display()
        )
    }
}

impl Opts {
    fn emit_cargo_output(&self) -> bool {
        self.verbosity >= 2
    }

    fn emit_cmd(&self) -> bool {
        self.verbosity >= 1
    }
}

#[derive(Debug, thiserror::Error)]
struct ExitError(i32);

impl fmt::Display for ExitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exiting with {}", self.0)
    }
}

impl Config {
    fn default_outcome_of_output(&self, output: &process::Output) -> TestOutcome {
        let status = output.status;
        let stdout_utf8 = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr_utf8 = String::from_utf8_lossy(&output.stderr).to_string();

        debug!(
            "status: {:?} stdout: {:?} stderr: {:?}",
            status, stdout_utf8, stderr_utf8
        );

        let saw_ice = stderr_utf8.contains("error: internal compiler error")
            || stderr_utf8.contains("' has overflowed its stack")
            || stderr_utf8.contains("error: the compiler unexpectedly panicked");

        let input = (self.args.regress, status.success());
        let result = match input {
            (RegressOn::Error, true) | (RegressOn::Success, false) => TestOutcome::Baseline,
            (RegressOn::Error, false) | (RegressOn::Success | RegressOn::NonError, true) => {
                TestOutcome::Regressed
            }
            (RegressOn::Ice, _) | (RegressOn::NonError, false) => {
                if saw_ice {
                    TestOutcome::Regressed
                } else {
                    TestOutcome::Baseline
                }
            }
            (RegressOn::NonIce, _) => {
                if saw_ice {
                    TestOutcome::Baseline
                } else {
                    TestOutcome::Regressed
                }
            }
        };
        debug!(
            "default_outcome_of_output: input: {:?} result: {:?}",
            input, result
        );
        result
    }
}

#[derive(Clone, Debug, ValueEnum)]
enum Access {
    Checkout,
    Github,
}

impl Access {
    fn repo(&self) -> Box<dyn RustRepositoryAccessor> {
        match self {
            Self::Checkout => Box::new(AccessViaLocalGit),
            Self::Github => Box::new(AccessViaGithub),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, ValueEnum)]
/// Customize what is treated as regression.
enum RegressOn {
    /// Marks test outcome as `Regressed` if and only if the `rustc`
    /// process reports a non-success status. This corresponds to when `rustc`
    /// has an internal compiler error (ICE) or when it detects an error in the
    /// input program.
    /// This covers the most common use case for `cargo-bisect-rustc` and is
    /// thus the default setting.
    Error,

    /// Marks test outcome as `Regressed` if and only if the `rustc`
    /// process reports a success status. This corresponds to when `rustc`
    /// believes it has successfully compiled the program. This covers the use
    /// case for when you want to bisect to see when a bug was fixed.
    Success,

    /// Marks test outcome as `Regressed` if and only if the `rustc`
    /// process issues a diagnostic indicating that an internal compiler error
    /// (ICE) occurred. This covers the use case for when you want to bisect to
    /// see when an ICE was introduced on a codebase that is meant to produce
    /// a clean error.
    Ice,

    /// Marks test outcome as `Regressed` if and only if the `rustc`
    /// process does not issue a diagnostic indicating that an internal
    /// compiler error (ICE) occurred. This covers the use case for when you
    /// want to bisect to see when an ICE was fixed.
    NonIce,

    /// Marks test outcome as `Baseline` if and only if the `rustc`
    /// process reports error status and does not issue any diagnostic
    /// indicating that an internal compiler error (ICE) occurred. This is the
    /// use case if the regression is a case where an ill-formed program has
    /// stopped being properly rejected by the compiler.
    /// (The main difference between this case and `success` is the handling of
    /// ICE: `success` assumes that ICE should be considered baseline;
    /// `non-error` assumes ICE should be considered a sign of a regression.)
    NonError,
}

impl RegressOn {
    fn must_process_stderr(self) -> bool {
        match self {
            RegressOn::Error | RegressOn::Success => false,
            RegressOn::NonError | RegressOn::Ice | RegressOn::NonIce => true,
        }
    }
}

struct Config {
    args: Opts,
    bounds: Bounds,
    rustup_tmp_path: PathBuf,
    toolchains_path: PathBuf,
    target: String,
    client: Client,
}

impl Config {
    fn from_args(args: Opts) -> anyhow::Result<Config> {
        let target = args.target.clone().unwrap_or_else(|| args.host.clone());

        let mut toolchains_path = home::rustup_home()?;

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

        let bounds = Bounds::from_args(&args)?;

        Ok(Config {
            args,
            bounds,
            target,
            toolchains_path,
            rustup_tmp_path,
            client: Client::new(),
        })
    }
}

// Application entry point
fn run() -> anyhow::Result<()> {
    env_logger::try_init()?;
    let args = match Cargo::try_parse() {
        Ok(Cargo::BisectRustc(args)) => args,
        Err(e) => match e.context().next() {
            None => {
                Cargo::parse();
                unreachable!()
            }
            _ => Opts::parse(),
        },
    };
    let cfg = Config::from_args(args)?;

    if let Some(ref bound) = cfg.args.install {
        cfg.install(bound)
    } else {
        cfg.bisect()
    }
}

impl Config {
    fn install(&self, bound: &Bound) -> anyhow::Result<()> {
        match *bound {
            Bound::Commit(ref sha) => {
                let sha = self.args.access.repo().commit(sha)?.sha;
                let mut t = Toolchain {
                    spec: ToolchainSpec::Ci {
                        commit: sha,
                        alt: self.args.alt,
                    },
                    host: self.args.host.clone(),
                    std_targets: vec![self.args.host.clone(), self.target.clone()],
                };
                t.std_targets.sort();
                t.std_targets.dedup();
                let dl_params = DownloadParams::for_ci(self);
                t.install(&self.client, &dl_params)?;
            }
            Bound::Date(date) => {
                let mut t = Toolchain {
                    spec: ToolchainSpec::Nightly { date },
                    host: self.args.host.clone(),
                    std_targets: vec![self.args.host.clone(), self.target.clone()],
                };
                t.std_targets.sort();
                t.std_targets.dedup();
                let dl_params = DownloadParams::for_nightly(self);
                t.install(&self.client, &dl_params)?;
            }
        }

        Ok(())
    }

    fn do_perf_search(&self, result: &BisectionResult) {
        let toolchain = &result.searched[result.found];
        match self.search_perf_builds(toolchain) {
            Ok(result) => {
                let bisection = result.bisection;
                let url = format!(
                    "https://github.com/rust-lang-ci/rust/commit/{}",
                    bisection.searched[bisection.found]
                )
                .red()
                .bold();
                eprintln!("Regression in {url}");

                // In case the bisected commit has been garbage-collected by github, we show its
                // additional context here.
                let context = &result.toolchain_descriptions[bisection.found];
                eprintln!("The PR introducing the regression in this rollup is {context}");
            }
            Err(e) => {
                eprintln!("ERROR: {e}");
            }
        }
    }

    // bisection entry point
    fn bisect(&self) -> anyhow::Result<()> {
        if let Bounds::Commits { start, end } = &self.bounds {
            let bisection_result = self.bisect_ci(start, end)?;
            self.print_results(&bisection_result);
            self.do_perf_search(&bisection_result);
        } else {
            let nightly_bisection_result = self.bisect_nightlies()?;
            self.print_results(&nightly_bisection_result);
            let nightly_regression =
                &nightly_bisection_result.searched[nightly_bisection_result.found];

            if let ToolchainSpec::Nightly { date } = nightly_regression.spec {
                let mut previous_date = date.pred_opt().unwrap();
                let working_commit = loop {
                    match Bound::Date(previous_date).sha() {
                        Ok(sha) => break sha,
                        Err(err)
                            if matches!(
                                err.downcast_ref::<DownloadError>(),
                                Some(DownloadError::NotFound(_)),
                            ) =>
                        {
                            eprintln!("missing nightly for {}", previous_date.format(YYYY_MM_DD));
                            previous_date = previous_date.pred_opt().unwrap();
                        }
                        Err(err) => return Err(err),
                    }
                };

                let bad_commit = Bound::Date(date).sha()?;
                eprintln!(
                    "looking for regression commit between {} and {}",
                    previous_date.format(YYYY_MM_DD),
                    date.format(YYYY_MM_DD),
                );

                let ci_bisection_result = self.bisect_ci_via(&working_commit, &bad_commit)?;

                self.print_results(&ci_bisection_result);
                self.do_perf_search(&ci_bisection_result);
                print_final_report(self, &nightly_bisection_result, &ci_bisection_result);
            }
        }

        Ok(())
    }
}

fn searched_range(
    cfg: &Config,
    searched_toolchains: &[Toolchain],
) -> (ToolchainSpec, ToolchainSpec) {
    let first_toolchain = searched_toolchains.first().unwrap().spec.clone();
    let last_toolchain = searched_toolchains.last().unwrap().spec.clone();

    match (&first_toolchain, &last_toolchain) {
        (ToolchainSpec::Ci { .. }, ToolchainSpec::Ci { .. }) => (first_toolchain, last_toolchain),

        _ => {
            // The searched_toolchains is a subset of the range actually
            // searched since they don't always include the complete bounds
            // due to `Config::bisect_nightlies` narrowing the range. Show the
            // true range of dates searched.
            match cfg.bounds {
                Bounds::SearchNightlyBackwards { end } => {
                    (first_toolchain, ToolchainSpec::Nightly { date: end })
                }
                Bounds::Commits { .. } => unreachable!("expected nightly bisect"),
                Bounds::Dates { start, end } => (
                    ToolchainSpec::Nightly { date: start },
                    ToolchainSpec::Nightly { date: end },
                ),
            }
        }
    }
}

impl Config {
    fn print_results(&self, bisection_result: &BisectionResult) {
        let BisectionResult {
            searched: toolchains,
            dl_spec,
            found,
        } = bisection_result;

        let (start, end) = searched_range(self, toolchains);

        eprintln!("searched toolchains {} through {}", start, end);

        if toolchains[*found] == *toolchains.last().unwrap() {
            // FIXME: Ideally the BisectionResult would contain the final result.
            // This ends up testing a toolchain that was already tested.
            // I believe this is one of the duplicates mentioned in
            // https://github.com/rust-lang/cargo-bisect-rustc/issues/85
            eprintln!("checking last toolchain to determine final result");
            let t = &toolchains[*found];
            let r = match t.install(&self.client, dl_spec) {
                Ok(()) => {
                    let outcome = t.test(self);
                    remove_toolchain(self, t, dl_spec);
                    // we want to fail, so a successful build doesn't satisfy us
                    match outcome {
                        TestOutcome::Baseline => Satisfies::No,
                        TestOutcome::Regressed => Satisfies::Yes,
                    }
                }
                Err(_) => {
                    let _ = t.remove(dl_spec);
                    Satisfies::Unknown
                }
            };
            match r {
                Satisfies::Yes => {}
                Satisfies::No | Satisfies::Unknown => {
                    eprintln!(
                        "error: The regression was not found. Expanding the bounds may help."
                    );
                    return;
                }
            }
        }

        let tc_found = format!("Regression in {}", toolchains[*found]);
        eprintln!();
        eprintln!();
        eprintln!("{}", "*".repeat(80).dimmed().bold());
        eprintln!("{}", tc_found.red());
        eprintln!("{}", "*".repeat(80).dimmed().bold());
        eprintln!();
    }
}

fn remove_toolchain(cfg: &Config, toolchain: &Toolchain, dl_params: &DownloadParams) {
    if cfg.args.preserve {
        // If `rustup toolchain link` was used to link to nightly, then even
        // with --preserve, the toolchain link should be removed, otherwise it
        // will go stale after 24 hours.
        let toolchain_dir = cfg.toolchains_path.join(toolchain.rustup_name());
        match fs::symlink_metadata(&toolchain_dir) {
            Ok(meta) => {
                #[cfg(windows)]
                let is_junction = {
                    use std::os::windows::fs::MetadataExt;
                    (meta.file_attributes() & 1024) != 0
                };
                #[cfg(not(windows))]
                let is_junction = false;
                if !meta.file_type().is_symlink() && !is_junction {
                    return;
                }
                debug!("removing linked toolchain {}", toolchain);
            }
            Err(e) => {
                debug!(
                    "remove_toolchain: cannot stat toolchain {}: {}",
                    toolchain, e
                );
                return;
            }
        }
    }
    if let Err(e) = toolchain.remove(dl_params) {
        debug!(
            "failed to remove toolchain {} in {}: {}",
            toolchain,
            cfg.toolchains_path.display(),
            e
        );
    }
}

fn print_final_report(
    cfg: &Config,
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

    eprintln!("{}", REPORT_HEADER.dimmed());
    eprintln!();

    let (start, end) = searched_range(cfg, nightly_toolchains);

    eprintln!("searched nightlies: from {} to {}", start, end);

    eprintln!("regressed nightly: {}", nightly_toolchains[*nightly_found],);

    eprintln!(
        "searched commit range: https://github.com/rust-lang/rust/compare/{0}...{1}",
        ci_toolchains.first().unwrap(),
        ci_toolchains.last().unwrap(),
    );

    eprintln!(
        "regressed commit: https://github.com/rust-lang/rust/commit/{}",
        ci_toolchains[*ci_found],
    );

    eprintln!();
    eprintln!("<details>");
    eprintln!(
        "<summary>bisected with <a href='{}'>cargo-bisect-rustc</a> v{}</summary>",
        env!("CARGO_PKG_REPOSITORY"),
        env!("CARGO_PKG_VERSION"),
    );
    eprintln!();
    eprintln!();
    if let Some(host) = option_env!("HOST") {
        eprintln!("Host triple: {}", host);
    }

    eprintln!("Reproduce with:");
    eprintln!("```bash");
    eprint!("cargo bisect-rustc ");
    for arg in env::args_os()
        .map(|arg| arg.to_string_lossy().into_owned())
        .skip_while(|arg| arg.ends_with("bisect-rustc"))
    {
        eprint!("{arg} ");
    }
    eprintln!();
    eprintln!("```");
    eprintln!("</details>");
}

struct NightlyFinderIter {
    start_date: GitDate,
    current_date: GitDate,
}

impl NightlyFinderIter {
    fn new(start_date: GitDate) -> Self {
        Self {
            start_date,
            current_date: start_date,
        }
    }
}

impl Iterator for NightlyFinderIter {
    type Item = GitDate;

    fn next(&mut self) -> Option<GitDate> {
        let current_distance = self.start_date - self.current_date;

        let jump_length = if current_distance.num_days() < 7 {
            // first week jump by two days
            2
        } else if current_distance.num_days() < 49 {
            // from 2nd to 7th week jump weekly
            7
        } else {
            // from 7th week jump by two weeks
            14
        };

        self.current_date = self.current_date - Duration::days(jump_length);
        Some(self.current_date)
    }
}

impl Config {
    fn install_and_test(
        &self,
        t: &Toolchain,
        dl_spec: &DownloadParams,
    ) -> Result<Satisfies, InstallError> {
        let regress = self.args.regress;
        let term_old = self.args.term_old.as_deref().unwrap_or_else(|| {
            if self.args.script.is_some() {
                match regress {
                    RegressOn::Error => "Script returned success",
                    RegressOn::Success => "Script returned error",
                    RegressOn::Ice => "Script did not ICE",
                    RegressOn::NonIce => "Script found ICE",
                    RegressOn::NonError => "Script returned error (no ICE)",
                }
            } else {
                match regress {
                    RegressOn::Error => "Successfully compiled",
                    RegressOn::Success => "Compile error",
                    RegressOn::Ice => "Did not ICE",
                    RegressOn::NonIce => "Found ICE",
                    RegressOn::NonError => "Compile error (no ICE)",
                }
            }
        });
        let term_new = self.args.term_new.as_deref().unwrap_or_else(|| {
            if self.args.script.is_some() {
                match regress {
                    RegressOn::Error => "Script returned error",
                    RegressOn::Success => "Script returned success",
                    RegressOn::Ice => "Script found ICE",
                    RegressOn::NonIce => "Script did not ICE",
                    RegressOn::NonError => "Script returned success or ICE",
                }
            } else {
                match regress {
                    RegressOn::Error => "Compile error",
                    RegressOn::Success => "Successfully compiled",
                    RegressOn::Ice => "Found ICE",
                    RegressOn::NonIce => "Did not ICE",
                    RegressOn::NonError => "Successfully compiled or ICE",
                }
            }
        });
        match t.install(&self.client, dl_spec) {
            Ok(()) => {
                let outcome = t.test(self);
                // we want to fail, so a successful build doesn't satisfy us
                let r = match outcome {
                    TestOutcome::Baseline => Satisfies::No,
                    TestOutcome::Regressed => Satisfies::Yes,
                };
                eprintln!(
                    "RESULT: {}, ===> {}",
                    t,
                    r.msg_with_context(term_old, term_new)
                );
                remove_toolchain(self, t, dl_spec);
                eprintln!();
                Ok(r)
            }
            Err(error) => {
                remove_toolchain(self, t, dl_spec);
                Err(error)
            }
        }
    }

    fn bisect_to_regression(&self, toolchains: &[Toolchain], dl_spec: &DownloadParams) -> usize {
        least_satisfying(toolchains, |t, remaining, estimate| {
            eprintln!(
                "{remaining} versions remaining to test after this (roughly {estimate} steps)"
            );
            self.install_and_test(t, dl_spec)
                .unwrap_or(Satisfies::Unknown)
        })
    }
}

impl Config {
    // nightlies branch of bisect execution
    fn bisect_nightlies(&self) -> anyhow::Result<BisectionResult> {
        if self.args.alt {
            bail!("cannot bisect nightlies with --alt: not supported");
        }

        let dl_spec = DownloadParams::for_nightly(self);

        // before this date we didn't have -std packages
        let end_at = NaiveDate::from_ymd_opt(2015, 10, 20).unwrap();
        // The date where a passing build is first found. This becomes
        // the new start point of the bisection range.
        let mut first_success = None;

        // nightly_date is the date we are currently testing to find the start
        // point. The loop below modifies nightly_date towards older dates
        // as it tries to find the starting point. It will become the basis
        // for setting first_success once a passing toolchain is found.
        //
        // last_failure is the oldest date where a regression was found while
        // walking backwards. This becomes the new endpoint of the bisection
        // range.
        let (mut nightly_date, mut last_failure) = match self.bounds {
            Bounds::SearchNightlyBackwards { end } => (end, end),
            Bounds::Commits { .. } => unreachable!(),
            Bounds::Dates { start, end } => (start, end),
        };

        let has_start = self.args.start.is_some();

        let mut nightly_iter = NightlyFinderIter::new(nightly_date);

        // this loop tests nightly toolchains to:
        // (1) validate that start date does not have regression (if defined on command line)
        // (2) identify a nightly date range for the bisection routine
        //
        // The tests here must be constrained to dates after 2015-10-20 (`end_at` date)
        // because -std packages were not available prior
        while nightly_date > end_at {
            let mut t = Toolchain {
                spec: ToolchainSpec::Nightly { date: nightly_date },
                host: self.args.host.clone(),
                std_targets: vec![self.args.host.clone(), self.target.clone()],
            };
            t.std_targets.sort();
            t.std_targets.dedup();
            if t.is_current_nightly() {
                eprintln!(
                    "checking {} from the currently installed default nightly \
                       toolchain as the last failure",
                    t
                );
            }

            eprintln!("checking the start range to find a passing nightly");
            match self.install_and_test(&t, &dl_spec) {
                Ok(r) => {
                    // If Satisfies::No, then the regression was not identified in this nightly.
                    // Break out of the loop and use this as the start date for the
                    // bisection range
                    if r == Satisfies::No {
                        first_success = Some(nightly_date);
                        break;
                    } else if has_start {
                        // If this date was explicitly defined on the command line &
                        // has regression, then this is an error in the test definition.
                        // The user must re-define the start date and try again
                        bail!(
                            "the start of the range ({}) must not reproduce the regression",
                            t
                        );
                    }
                    last_failure = nightly_date;
                    nightly_date = nightly_iter.next().unwrap();
                }
                Err(InstallError::NotFound { .. }) => {
                    // go back just one day, presumably missing a nightly
                    nightly_date = nightly_date.pred_opt().unwrap();
                    eprintln!(
                        "*** unable to install {}. roll back one day and try again...",
                        t
                    );
                    if has_start {
                        bail!("could not find {}", t);
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }

        let first_success = first_success.context("could not find a nightly that built")?;

        // confirm that the end of the date range has the regression
        let mut t_end = Toolchain {
            spec: ToolchainSpec::Nightly { date: last_failure },
            host: self.args.host.clone(),
            std_targets: vec![self.args.host.clone(), self.target.clone()],
        };
        t_end.std_targets.sort();
        t_end.std_targets.dedup();

        eprintln!("checking the end range to verify it does not pass");
        let result_nightly = self.install_and_test(&t_end, &dl_spec)?;
        // The regression was not identified in this nightly.
        if result_nightly == Satisfies::No {
            bail!(
                "the end of the range ({}) does not reproduce the regression",
                t_end
            );
        }

        let toolchains = toolchains_between(
            self,
            ToolchainSpec::Nightly {
                date: first_success,
            },
            ToolchainSpec::Nightly { date: last_failure },
        );

        let found = self.bisect_to_regression(&toolchains, &dl_spec);

        Ok(BisectionResult {
            dl_spec,
            searched: toolchains,
            found,
        })
    }
}

fn toolchains_between(cfg: &Config, a: ToolchainSpec, b: ToolchainSpec) -> Vec<Toolchain> {
    match (a, b) {
        (ToolchainSpec::Nightly { date: a }, ToolchainSpec::Nightly { date: b }) => {
            let mut toolchains = Vec::new();
            let mut date = a;
            let mut std_targets = vec![cfg.args.host.clone(), cfg.target.clone()];
            std_targets.sort();
            std_targets.dedup();
            while date <= b {
                let t = Toolchain {
                    spec: ToolchainSpec::Nightly { date },
                    host: cfg.args.host.clone(),
                    std_targets: std_targets.clone(),
                };
                toolchains.push(t);
                date = date.succ_opt().unwrap();
            }
            toolchains
        }
        _ => unimplemented!(),
    }
}

impl Config {
    // CI branch of bisect execution
    fn bisect_ci(&self, start: &str, end: &str) -> anyhow::Result<BisectionResult> {
        eprintln!("bisecting ci builds starting at {start}, ending at {end}");
        self.bisect_ci_via(start, end)
    }

    fn bisect_ci_via(&self, start_sha: &str, end_sha: &str) -> anyhow::Result<BisectionResult> {
        let access = self.args.access.repo();
        let start = access.commit(start_sha)?;
        let end = access.commit(end_sha)?;
        let assert_by_bors = |c: &Commit| -> anyhow::Result<()> {
            if c.committer.name != BORS_AUTHOR {
                bail!(
                    "Expected author {} to be {BORS_AUTHOR} for {}.\n \
                     Make sure specified commits are on the master branch \
                     and refer to a bors merge commit!",
                    c.committer.name,
                    c.sha
                );
            }
            Ok(())
        };
        assert_by_bors(&start)?;
        assert_by_bors(&end)?;
        let commits = access.commits(start_sha, &end.sha)?;

        let Some(last) = commits.last() else {
            bail!("expected at least one commit");
        };
        if !last.sha.starts_with(&end.sha) {
            bail!(
                "expected the last commit to be {end_sha}, but got {}",
                last.sha
            );
        }

        commits.iter().zip(commits.iter().skip(1)).all(|(a, b)| {
            let sorted_by_date = a.date <= b.date;
            assert!(
                sorted_by_date,
                "commits must chronologically ordered,\
                                 but {:?} comes after {:?}",
                a, b
            );
            sorted_by_date
        });

        for (j, commit) in commits.iter().enumerate() {
            eprintln!(
                "  commit[{}] {}: {}",
                j,
                commit.date,
                commit.summary.split('\n').next().unwrap()
            )
        }

        self.bisect_ci_in_commits(start_sha, &end.sha, commits)
    }

    fn bisect_ci_in_commits(
        &self,
        start: &str,
        end: &str,
        mut commits: Vec<Commit>,
    ) -> anyhow::Result<BisectionResult> {
        let dl_spec = DownloadParams::for_ci(self);
        commits.retain(|c| today() - c.date < Duration::days(167));

        if commits.is_empty() {
            bail!(
                "no CI builds available between {} and {} within last 167 days",
                start,
                end
            );
        }

        if let Some(c) = commits.last() {
            if !c.sha.starts_with(end) {
                bail!("expected to end with {}, but ended with {}", end, c.sha);
            }
        }

        eprintln!("validated commits found, specifying toolchains");
        eprintln!();

        let toolchains = commits
            .into_iter()
            .map(|commit| {
                let mut t = Toolchain {
                    spec: ToolchainSpec::Ci {
                        commit: commit.sha,
                        alt: self.args.alt,
                    },
                    host: self.args.host.clone(),
                    std_targets: vec![self.args.host.clone(), self.target.clone()],
                };
                t.std_targets.sort();
                t.std_targets.dedup();
                t
            })
            .collect::<Vec<_>>();

        if !toolchains.is_empty() {
            // validate commit at start of range
            eprintln!("checking the start range to verify it passes");
            let start_range_result = self.install_and_test(&toolchains[0], &dl_spec)?;
            if start_range_result == Satisfies::Yes {
                bail!(
                    "the commit at the start of the range ({}) includes the regression",
                    &toolchains[0]
                );
            }

            // validate commit at end of range
            eprintln!("checking the end range to verify it does not pass");
            let end_range_result =
                self.install_and_test(&toolchains[toolchains.len() - 1], &dl_spec)?;
            if end_range_result == Satisfies::No {
                bail!(
                    "the commit at the end of the range ({}) does not reproduce the regression",
                    &toolchains[toolchains.len() - 1]
                );
            }
        }

        let found = self.bisect_to_regression(&toolchains, &dl_spec);

        Ok(BisectionResult {
            searched: toolchains,
            found,
            dl_spec,
        })
    }

    fn search_perf_builds(&self, toolchain: &Toolchain) -> anyhow::Result<PerfBisectionResult> {
        eprintln!("Attempting to search unrolled perf builds");
        let Toolchain {
            spec: ToolchainSpec::Ci { commit, .. },
            ..
        } = toolchain
        else {
            bail!("not a ci commit");
        };
        let summary = get_commit(commit)?.summary;
        if !summary.starts_with("Auto merge of #") && !summary.contains("Rollup of") {
            bail!("not a rollup pr");
        }
        let pr = summary.split(' ').nth(3).unwrap();
        // remove '#'
        let pr = pr.chars().skip(1).collect::<String>();
        let comments = get_pr_comments(&pr)?;
        let perf_comment = comments
            .iter()
            .filter(|c| c.user.login == "rust-timer")
            .find(|c| c.body.contains("Perf builds for each rolled up PR"))
            .context("couldn't find perf build comment")?;
        let context = extract_perf_builds(&perf_comment.body)?;
        let short_sha = context
            .builds
            .iter()
            .map(|sha| sha.chars().take(8).collect())
            .collect::<Vec<String>>();
        eprintln!("Found commits {short_sha:?}");

        let bisection = self.linear_in_commits(&context.builds)?;
        Ok(PerfBisectionResult {
            bisection,
            toolchain_descriptions: context.descriptions,
        })
    }

    fn linear_in_commits(&self, commits: &[&str]) -> anyhow::Result<BisectionResult> {
        let dl_spec = DownloadParams::for_ci(self);

        let toolchains = commits
            .into_iter()
            .map(|commit| {
                let mut t = Toolchain {
                    spec: ToolchainSpec::Ci {
                        commit: commit.to_string(),
                        alt: self.args.alt,
                    },
                    host: self.args.host.clone(),
                    std_targets: vec![self.args.host.clone(), self.target.clone()],
                };
                t.std_targets.sort();
                t.std_targets.dedup();
                t
            })
            .collect::<Vec<_>>();

        let Some(found) = toolchains.iter().position(|t| {
            self.install_and_test(t, &dl_spec)
                .unwrap_or(Satisfies::Unknown)
                == Satisfies::Yes
        }) else {
            bail!("none of the toolchains satisfied the predicate");
        };

        Ok(BisectionResult {
            searched: toolchains,
            found,
            dl_spec,
        })
    }
}

#[derive(Clone)]
struct BisectionResult {
    searched: Vec<Toolchain>,
    found: usize,
    dl_spec: DownloadParams,
}

/// The results of a bisection through the unrolled perf builds in a rollup:
/// - the regular bisection results
/// - a description of the rolled-up PRs for clearer diagnostics, in case the bisected commit
///   doesn't exist anymore on github.
#[derive(Clone)]
struct PerfBisectionResult {
    bisection: BisectionResult,
    toolchain_descriptions: Vec<String>,
}

fn main() {
    if let Err(err) = run() {
        match err.downcast::<ExitError>() {
            Ok(ExitError(code)) => process::exit(code),
            Err(err) => {
                let error_str = "ERROR:".red().bold();
                eprintln!("{} {:?}", error_str, err);
                process::exit(1);
            }
        }
    }
}

/// An in-order mapping from perf build SHA to its description.
struct PerfBuildsContext<'a> {
    builds: Vec<&'a str>,
    descriptions: Vec<String>,
}

/// Extracts the commits posted by the rust-timer bot on rollups, for unrolled perf builds, with
/// their associated context: the PR number and title if available.
///
/// We're looking for a commit SHA, in a comment whose format has changed (and could change in the
/// future), for example:
/// - v1: https://github.com/rust-lang/rust/pull/113014#issuecomment-1605868471
/// - v2, the current: https://github.com/rust-lang/rust/pull/113105#issuecomment-1610393473
///
/// The SHA comes in later columns, so we'll look for a 40-char hex string and give priority to the
/// last we find (to avoid possible conflicts with commits in the PR title column).
///
/// Depending on how recent the perf build commit is, it may have been garbage-collected by github:
/// perf-builds are force pushed to the `try-perf` branch, and accessing that commit can
/// 404. Therefore, we try to map back from that commit to the rolled-up PR present in the list of
/// unrolled builds.
fn extract_perf_builds(body: &str) -> anyhow::Result<PerfBuildsContext<'_>> {
    let mut builds = Vec::new();
    let mut descriptions = Vec::new();

    let sha_regex = RegexBuilder::new(r"([0-9a-f]{40})")
        .case_insensitive(true)
        .build()?;
    for line in body
        .lines()
        // Only look at the lines of the unrolled perf builds table.
        .filter(|l| l.starts_with("|#"))
    {
        // Get the last SHA we find, to prioritize the 3rd or 2nd columns.
        let sha = sha_regex
            .find_iter(line)
            .last()
            .and_then(|m| Some(m.as_str()));

        // If we did find one, we try to extract the associated description.
        let Some(sha) = sha else { continue };

        let mut description = String::new();

        // In v1 and v2, we know that the first column is the PR number.
        //
        // In the unlikely event it's missing because of a parsing discrepancy, we don't want to
        // ignore it, and ask for feedback: we always want to have *some* context per PR, matching
        // the number of SHAs we found.
        let Some(pr) = line.split('|').nth(1) else {
            bail!("Couldn't get rolled-up PR number for SHA {sha}, please open an issue.");
        };

        description.push_str(pr);

        // The second column could be a link to the commit (which we don't want in the description),
        // or the PR title (which we want).
        if let Some(title) = line.split('|').nth(2) {
            // For v1, this column would contain the commit, and we won't have the PR title
            // anywhere. So we try to still give some context for that older format: if the column
            // contains the SHA, we don't add that to the description.
            if !title.contains(sha) {
                description.push_str(": ");
                description.push_str(title);
            }
        }

        builds.push(sha);
        descriptions.push(description);
    }

    Ok(PerfBuildsContext {
        builds,
        descriptions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nightly_finder_iterator() {
        let start_date = NaiveDate::from_ymd_opt(2019, 01, 01).unwrap();

        let iter = NightlyFinderIter::new(start_date);

        for (date, i) in iter.zip([2, 4, 6, 8, 15, 22, 29, 36, 43, 50, 64, 78]) {
            assert_eq!(start_date - Duration::days(i), date)
        }
    }

    #[test]
    fn test_validate_dir() {
        let current_dir = ".";
        assert!(validate_dir(current_dir).is_ok());
        let main = "src/main.rs";
        assert!(
            validate_dir(main).is_err(),
            "{}",
            validate_dir(main).unwrap_err()
        )
    }

    // Ensure the first version of the comment posted by the perf-bot works
    #[test]
    fn test_perf_builds_v1_format() {
        // Body extracted from this v1 comment
        // https://github.com/rust-lang/rust/pull/113014#issuecomment-1605868471
        let body = "📌 Perf builds for each rolled up PR:

|PR# | Perf Build Sha|
|----|:-----:|
|#113009|[05b07dad146a6d43ead9bcd1e8bc10cbd017a5f5](https://github.com/rust-lang-ci/rust/commit/05b07dad146a6d43ead9bcd1e8bc10cbd017a5f5)|
|#113008|[581913b6789370def5158093b799baa6d4d875eb](https://github.com/rust-lang-ci/rust/commit/581913b6789370def5158093b799baa6d4d875eb)|
|#112956|[e294bd3827eb2e878167329648f3c8178ef344e7](https://github.com/rust-lang-ci/rust/commit/e294bd3827eb2e878167329648f3c8178ef344e7)|
|#112950|[0ed6ba504649ca1cb2672572b4ab41acfb06c86c](https://github.com/rust-lang-ci/rust/commit/0ed6ba504649ca1cb2672572b4ab41acfb06c86c)|
|#112937|[18e108ab85b78e6966c5b5bdadfd5b8efeadf080](https://github.com/rust-lang-ci/rust/commit/18e108ab85b78e6966c5b5bdadfd5b8efeadf080)|


*previous master*: [f7ca9df695](https://github.com/rust-lang-ci/rust/commit/f7ca9df69549470541fbf542f87a03eb9ed024b6)

In the case of a perf regression, run the following command for each PR you suspect might be the cause: `@rust-timer build $SHA`
<!-- rust-timer: rollup -->";
        let context =
            extract_perf_builds(body).expect("extracting perf builds context on v1 format failed");
        assert_eq!(
            vec![
                "05b07dad146a6d43ead9bcd1e8bc10cbd017a5f5",
                "581913b6789370def5158093b799baa6d4d875eb",
                "e294bd3827eb2e878167329648f3c8178ef344e7",
                "0ed6ba504649ca1cb2672572b4ab41acfb06c86c",
                "18e108ab85b78e6966c5b5bdadfd5b8efeadf080",
            ],
            context.builds,
        );
        assert_eq!(
            vec!["#113009", "#113008", "#112956", "#112950", "#112937",],
            context.descriptions,
        );
    }

    // Ensure the second version of the comment posted by the perf-bot works
    #[test]
    fn test_perf_builds_v2_format() {
        // Body extracted from this v2 comment
        // https://github.com/rust-lang/rust/pull/113105#issuecomment-1610393473
        let body = "📌 Perf builds for each rolled up PR:

| PR# | Message | Perf Build Sha |
|----|----|:-----:|
|#112207|Add trustzone and virtualization target features for aarch3…|`bbec6d6e413aa144c8b9346da27a0f2af299cbeb` ([link](https://github.com/rust-lang-ci/rust/commit/bbec6d6e413aa144c8b9346da27a0f2af299cbeb))|
|#112454|Make compiletest aware of targets without dynamic linking|`70b67c09ead52f4582471650202b1a189821ed5f` ([link](https://github.com/rust-lang-ci/rust/commit/70b67c09ead52f4582471650202b1a189821ed5f))|
|#112628|Allow comparing `Box`es with different allocators|`3043f4e577f41565443f38a6a16b7a1a08b063ad` ([link](https://github.com/rust-lang-ci/rust/commit/3043f4e577f41565443f38a6a16b7a1a08b063ad))|
|#112692|Provide more context for `rustc +nightly -Zunstable-options…|`4ab6f33fd50237b105999cc6d32d85cce5dad61a` ([link](https://github.com/rust-lang-ci/rust/commit/4ab6f33fd50237b105999cc6d32d85cce5dad61a))|
|#112972|Make `UnwindAction::Continue` explicit in MIR dump|`e1df9e306054655d7d41ec1ad75ade5d76a6888d` ([link](https://github.com/rust-lang-ci/rust/commit/e1df9e306054655d7d41ec1ad75ade5d76a6888d))|
|#113020|Add tests impl via obj unless denied|`affe009b94eba41777cf02997b1780e50445d6af` ([link](https://github.com/rust-lang-ci/rust/commit/affe009b94eba41777cf02997b1780e50445d6af))|
|#113084|Simplify some conditions|`0ce4618dbf5810aabb389edd4950c060b6b4d049` ([link](https://github.com/rust-lang-ci/rust/commit/0ce4618dbf5810aabb389edd4950c060b6b4d049))|
|#113103|Normalize types when applying uninhabited predicate.|`241cd8cd818cdc865cdf02f0c32a40081420b772` ([link](https://github.com/rust-lang-ci/rust/commit/241cd8cd818cdc865cdf02f0c32a40081420b772))|


*previous master*: [5ea6668646](https://github.com/rust-lang-ci/rust/commit/5ea66686467d3ec5f8c81570e7f0f16ad8dd8cc3)

In the case of a perf regression, run the following command for each PR you suspect might be the cause: `@rust-timer build $SHA`
<!-- rust-timer: rollup -->";
        let context =
            extract_perf_builds(body).expect("extracting perf builds context on v2 format failed");
        assert_eq!(
            vec![
                "bbec6d6e413aa144c8b9346da27a0f2af299cbeb",
                "70b67c09ead52f4582471650202b1a189821ed5f",
                "3043f4e577f41565443f38a6a16b7a1a08b063ad",
                "4ab6f33fd50237b105999cc6d32d85cce5dad61a",
                "e1df9e306054655d7d41ec1ad75ade5d76a6888d",
                "affe009b94eba41777cf02997b1780e50445d6af",
                "0ce4618dbf5810aabb389edd4950c060b6b4d049",
                "241cd8cd818cdc865cdf02f0c32a40081420b772",
            ],
            context.builds,
        );
        assert_eq!(
            vec![
                "#112207: Add trustzone and virtualization target features for aarch3…",
                "#112454: Make compiletest aware of targets without dynamic linking",
                "#112628: Allow comparing `Box`es with different allocators",
                "#112692: Provide more context for `rustc +nightly -Zunstable-options…",
                "#112972: Make `UnwindAction::Continue` explicit in MIR dump",
                "#113020: Add tests impl via obj unless denied",
                "#113084: Simplify some conditions",
                "#113103: Normalize types when applying uninhabited predicate.",
            ],
            context.descriptions,
        );
    }
}

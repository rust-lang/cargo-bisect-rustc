//! Source fallback for CI toolchains using a persistent bootstrap build directory.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail, ensure};
use git2::Repository;

use crate::toolchains::{DownloadParams, Toolchain, ToolchainSpec};

fn run(cmd: &mut Command) -> anyhow::Result<()> {
    eprintln!("running {cmd:?}");
    let status = cmd.status().with_context(|| format!("running {cmd:?}"))?;
    ensure!(status.success(), "{cmd:?} failed with {status}");
    Ok(())
}

fn checkout(repo: &Repository, worktree: Option<&Path>, commit: &str) -> anyhow::Result<PathBuf> {
    let path = if let Some(path) = worktree {
        let path = env::current_dir()?.join(path);
        if !path.join(".git").exists() {
            run(Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(["worktree", "add", "--detach"])
                .arg(&path)
                .arg(commit))?;
        }
        let worktree = Repository::open(&path)?;
        ensure!(
            worktree.is_worktree()
                && fs::canonicalize(worktree.commondir())? == fs::canonicalize(repo.commondir())?,
            "--source-cache must be a worktree of the Rust repository"
        );
        path
    } else {
        repo.workdir()
            .context("bare Rust repositories require --source-cache")?
            .to_owned()
    };
    let path = fs::canonicalize(path)?;
    run(Command::new("git")
        .arg("-C")
        .arg(&path)
        .args(["checkout", "--detach", commit]))?;
    Ok(path)
}

fn quote(path: &Path) -> anyhow::Result<String> {
    // JSON strings are also valid TOML basic strings for filesystem paths.
    Ok(serde_json::to_string(
        path.to_str().context("source build paths must be UTF-8")?,
    )?)
}

fn bootstrap(repo: &Path, config: &Path, paths: &[&str]) -> anyhow::Result<()> {
    // Older LLVM headers rely on transitive includes removed in GCC 15.
    let mut cxxflags = env::var_os("CXXFLAGS").unwrap_or_default();
    cxxflags.push(" -include cstdint");
    run(Command::new("python3")
        .current_dir(repo)
        .arg("x.py")
        .arg("install")
        .arg("--config")
        .arg(config)
        .args(["--stage", "2"])
        .args(paths)
        .env("CXXFLAGS", cxxflags)
        // Do not inherit the toolchain being tested or the reproducer's flags.
        .env_remove("RUSTUP_TOOLCHAIN")
        .env_remove("RUSTC")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("DESTDIR")
        .env_remove("CARGO_TARGET_DIR"))
}

fn config(host: &str, build_dir: &Path, prefix: &Path) -> anyhow::Result<String> {
    let host = serde_json::to_string(host)?;
    Ok(format!(
        "[build]\nbuild = {host}\nhost = [{host}]\ntarget = [{host}]\n\
         build-dir = {}\nextended = true\ntools = [\"cargo\"]\ndocs = false\n\
         [install]\nprefix = {}\nsysconfdir = \"etc\"\n\
         [llvm]\ndownload-ci-llvm = false\n\
         [rust]\nchannel = \"nightly\"\ndownload-rustc = false\nlld = false\n",
        quote(build_dir)?,
        quote(prefix)?,
    ))
}

pub(crate) fn install(t: &Toolchain, params: &DownloadParams) -> anyhow::Result<()> {
    let ToolchainSpec::Ci { commit, alt } = &t.spec else {
        bail!("source builds require a CI commit");
    };
    ensure!(!alt, "source fallback does not support --alt");
    ensure!(
        t.host == env!("HOST") && t.std_targets.iter().all(|target| target == &t.host),
        "source fallback currently supports only the native host and standard library"
    );
    ensure!(
        params.source_components_supported,
        "source fallback supports rustc, rust-std and cargo; remove extra component options"
    );

    let source = Repository::open(crate::git::repo_path())?;
    let repo = checkout(&source, params.source_cache.as_deref(), commit)?;
    eprintln!(
        "CI artifacts missing for {commit}; building from source in {}",
        repo.display()
    );
    let build_dir = repo.join("build");
    fs::create_dir_all(&build_dir)?;
    let config_path = build_dir.join("bisect-rustc.toml");
    let tmp = tempfile::tempdir_in(&params.tmp_dir)?;
    let prefix = fs::canonicalize(tmp.path())?;
    fs::write(&config_path, config(&t.host, &build_dir, &prefix)?)?;
    let mut paths = vec!["compiler/rustc", "library/std"];
    if params.includes_cargo() {
        paths.push("cargo");
    }
    bootstrap(&repo, &config_path, &paths)?;
    ensure!(
        prefix
            .join("bin")
            .join(format!("rustc{}", env::consts::EXE_SUFFIX))
            .is_file(),
        "source installation did not produce rustc"
    );
    if params.includes_cargo() {
        ensure!(
            prefix
                .join("bin")
                .join(format!("cargo{}", env::consts::EXE_SUFFIX))
                .is_file(),
            "source installation did not produce the requested cargo"
        );
    }
    fs::rename(&prefix, params.install_dir.join(t.rustup_name()))?;
    Ok(())
}

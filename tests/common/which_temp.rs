use tempfile::TempDir;
use std::fs::{DirBuilder};
use std::path::{Path, PathBuf};

pub trait WhichTempDirectory {
    type Root;
    fn root() -> Result<Self::Root, failure::Error>;
    fn target(root: &Self::Root) -> &Path;
    fn dir_builder() -> DirBuilder;
}

// If you change this to `GenerateIntoSlashTemp`, you get a fixed directory
// rooted at `/tmp/` so that e.g. inspect the generated code, or even run
// cargo-bisect-rustc yourself there.
//
// Doing so has two drawbacks: 1. You need to clean-up the
// generated directory yourself, and 2. You risk race conditions with
// concurrent test runs.
//
// If you cange this to `GenerateIntoFreshTemp`, you get a fresh directory
// (rooted at whatever is returned from `tempfile::tempdir()`). This reduces
// race conditions (note that `cargo-bisect-rustc` still stores data in shared
// locations like `~/.rustup`, so races can still arise) and allows the test
// suite to clean up the directory itself.
//
// The `WhichTempDir` type is meant to always implement the `WhichTempDirectory`
// trait.
pub(crate) type WhichTempDir = GenerateIntoFreshTemp;

/// Using `GenerateIntoFreshTemp` yields a fresh directory in some
/// system-dependent temporary space. This fresh directory will be autoamtically
/// cleaned up after each test run, regardless of whether the test succeeded or
/// failed.
///
/// This approach has the following drawbacks: 1. the directories are fresh, so
/// any state stored in the local directory (like a rust.git checkout) will need
/// to be re-downloaded on each test run, even when using the `--preserve`
/// parameter, and 2. the directories are automatically deleted, which means you
/// cannot readily inspect the generated source code when trying to debug a test
/// failure.
///
/// The above two drawbacks can be sidestepped by using `GenerateIntoSlashTemp`
/// instead; see below.
pub(crate) struct GenerateIntoFreshTemp;

/// Using `GenerateIntoSlashTemp` yields a fixed directory rooted at `/tmp/` so
/// that you can inspect the generated code, or even run cargo-bisect-rustc
/// yourself there.
///
/// This approach has two drawbacks: 1. You need to clean-up the generated
/// directory yourself, and 2. You risk race conditions with concurrent test
/// runs.
pub(crate) struct GenerateIntoSlashTemp;

impl WhichTempDirectory for GenerateIntoSlashTemp {
    type Root = PathBuf;
    fn root() -> Result<Self::Root, failure::Error> {
        Ok(PathBuf::from("/tmp"))
    }
    fn target(root: &PathBuf) -> &Path {
        root
    }
    fn dir_builder() -> DirBuilder {
        // make recursive DirBuilder so that we do not error on pre-existing
        // directory.
        let mut b = DirBuilder::new();
        b.recursive(true);
        b
    }
}

impl WhichTempDirectory for GenerateIntoFreshTemp {
    type Root = TempDir;
    fn root() -> Result<Self::Root, failure::Error> {
        Ok(tempfile::tempdir()?)
    }
    fn target(root: &TempDir) -> &Path {
        root.path()
    }
    fn dir_builder() -> DirBuilder {
        DirBuilder::new()
    }
}

# Changelog

## v0.5.3

### Added

- Support specifying the path to a rust-lang/rust clone at runtime with `RUST_SRC_REPO`

### Changed

- Make `--with-cargo` the default to allow bisecting past changes in rustc options. Add `--without-cargo` flag to use the old behavior.
- Use an anonymous remote that always points to rust-lang/rust when refreshing repository

### Fixed

- Add nightly start and end date validations against the current date – previously would attempt to install nightly even if date was in the future
- Verify that `--test-dir` is a directory instead of assuming it is and then panicking

## v0.5.2

- Fix: revert the revised internal compiler error definition in commit a3891cdd26d1c5d35257c351c7c86fa7e72604bb

## v0.5.1

- Fix: Windows build fails due to dependency of `console` dependency issue.  Updated `winapi-util` package to v0.1.5 (from 0.1.2)

## v0.5.0

- New: include compiler crashes in ICE regression definition
- New: ANSI escape code colored standard stream output
- New: Add bisect-rustc version to the final report
- New: Add host triple to the final report
- New: Add command line args to reproduce the reporter's bisect-rustc tests to final report
- Fix: end date reporting when `--end` option used without `--start` option
- Updated: Standard stream reporting format for improved readability during execution
- Updated: Final report instructions for regression reporting
- Updated: Eliminated Markdown elements in the final report that are not typically included in rust-lang/rust issues by reporting users

## v0.4.1

- Fix: bug on git commit retrieval from local rust git repository when `--end` commit is not specified
- Fix: bug on git commit retrieval from GitHub API when `--end` commit is not specified
- Updated dependencies
- rustfmt source code

## v0.4.0

- Add support for GitHub API queries for Rust commit history
- Add support for `--regress=non-ice` regression definition
- Add support for `--script` arguments
- Fix duplicated start date range pulls/checks
- Reformat standard stream reporting

## v0.3.0

- Transition to Rustlang edition 2018
- Add test stage that can process output to decide outcome based on more subtle predicate than just `exit_status.success()`
- Add support for optional `--regress=ice` regression testing definition (default is `--regress=error`)
- Add support for optional `--regress=success` regression testing definition (default is `--regress=error`)
- Add support for optional `--regress=non-error` regression testing definition (default is `--regress=error`)
- Update the `remove` function to use an explicit `bisector` string at the beginning of the path name
- Update the `remove` function to guard against deleting state not managed by `cargo-bisect-rustc`
- Edit short and long help strings to fit on a single line
- Fix: support reuse of an already installed nightly, previously we would unconditionally fail the run

## v0.2.1

- Fix: refactor date bounds to assume that start date equals end date at the beginning of testing with `--end` option only

## v0.2.0

- Add automated regression report generation at the end of the test runs
- Add validation of date bounds
- Updated dependencies to avoid yanked dependency versions
- Improve documentation: Add documentation on how to list bors' commits for bisections to a PR
- Improve documentation: Update tutorial

## v0.1.0

- initial release

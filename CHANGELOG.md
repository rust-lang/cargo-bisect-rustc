# Changelog

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

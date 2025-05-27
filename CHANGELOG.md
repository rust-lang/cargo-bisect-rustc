# Changelog

## v0.6.10
[v0.6.9...v0.6.10](https://github.com/rust-lang/cargo-bisect-rustc/compare/v0.6.9...v0.6.10)

### Added
- Added the `--pretend-to-be-stable` flag.
  [#335](https://github.com/rust-lang/cargo-bisect-rustc/pull/335)
- Documented how to bisect an individual clippy warning.
  [#368](https://github.com/rust-lang/cargo-bisect-rustc/pull/368)
- Documented another example of a hanging compilation.
  [#374](https://github.com/rust-lang/cargo-bisect-rustc/pull/374)

### Changed
- Print the command that is run with `--verbose`.
  [#361](https://github.com/rust-lang/cargo-bisect-rustc/pull/361)
- Updated all dependencies.
  [#383](https://github.com/rust-lang/cargo-bisect-rustc/pull/383)
- Unrolled CI builds have moved from the `rust-lang-ci/rust` repository to the `rust-lang/rust` repository.
  [#381](https://github.com/rust-lang/cargo-bisect-rustc/pull/381)

### Fixed
- Fixed printing of args in the final report.
  [#356](https://github.com/rust-lang/cargo-bisect-rustc/pull/356)
- Fixed so that `cargo-bisect-rustc` can be run with the hyphen after `cargo` or a space.
  [#362](https://github.com/rust-lang/cargo-bisect-rustc/pull/362)

## v0.6.9

### Added
- Added flags `--term-old` and `--term-new` to allow custom messages when bisecting a regression.
  [#330](https://github.com/rust-lang/cargo-bisect-rustc/pull/330)
  [#339](https://github.com/rust-lang/cargo-bisect-rustc/pull/339)


### Changed
- Updated dependencies.
  [#314](https://github.com/rust-lang/cargo-bisect-rustc/pull/314)
  [#313](https://github.com/rust-lang/cargo-bisect-rustc/pull/313)
  [#315](https://github.com/rust-lang/cargo-bisect-rustc/pull/315)
  [#319](https://github.com/rust-lang/cargo-bisect-rustc/pull/319)
  [#326](https://github.com/rust-lang/cargo-bisect-rustc/pull/326)
  [#327](https://github.com/rust-lang/cargo-bisect-rustc/pull/327)
  [#329](https://github.com/rust-lang/cargo-bisect-rustc/pull/329)
  [#340](https://github.com/rust-lang/cargo-bisect-rustc/pull/340)
- No longer defaults to cross-compile mode when `--target` is not specified. This more closely matches `cargo`'s behavior, which can affect reproducability.
  [#323](https://github.com/rust-lang/cargo-bisect-rustc/pull/323)
- Removed LTO and stripping of building `cargo-bisect-rustc` itself.
  [#334](https://github.com/rust-lang/cargo-bisect-rustc/pull/334)

### Fixed
- Don't assume the date before the regressed nightly is the good nightly if there are missing nightlies.
  [#320](https://github.com/rust-lang/cargo-bisect-rustc/pull/320)
- Fixed building `cargo-bisect-rustc` itself to avoid unnecessary build-script rebuilds.
  [#324](https://github.com/rust-lang/cargo-bisect-rustc/pull/324)
- Fixed doc-change example documentation.
  [#336](https://github.com/rust-lang/cargo-bisect-rustc/pull/336)
- Replaced a panic with an error message if a given SHA commit is not from bors using the GitHub backend.
  [#318](https://github.com/rust-lang/cargo-bisect-rustc/pull/318)
- Fixed determination of what the latest nightly is when `--end` is not specified, and it is past UTC midnight, but the release process has not yet finished.
  [#325](https://github.com/rust-lang/cargo-bisect-rustc/pull/325)
- Fixed panic with `--by-commit` but no `--start`.
  [#325](https://github.com/rust-lang/cargo-bisect-rustc/pull/325)

## v0.6.8

### Added
- Added documentation for `--alt` builds.
  [#289](https://github.com/rust-lang/cargo-bisect-rustc/pull/289)

### Changed
- ❗️ Changed the default access method to "github", meaning it will use the GitHub API to fetch commit information instead of using a local git clone. See the [access documentation](https://rust-lang.github.io/cargo-bisect-rustc/rust-src-repo.html) for more information.
  [#307](https://github.com/rust-lang/cargo-bisect-rustc/pull/307)
- Updated dependencies.
  [#290](https://github.com/rust-lang/cargo-bisect-rustc/pull/290)
  [#291](https://github.com/rust-lang/cargo-bisect-rustc/pull/291)
  [#296](https://github.com/rust-lang/cargo-bisect-rustc/pull/296)
  [#302](https://github.com/rust-lang/cargo-bisect-rustc/pull/302)
  [#301](https://github.com/rust-lang/cargo-bisect-rustc/pull/301)
  [#300](https://github.com/rust-lang/cargo-bisect-rustc/pull/300)
  [#304](https://github.com/rust-lang/cargo-bisect-rustc/pull/304)
  [#305](https://github.com/rust-lang/cargo-bisect-rustc/pull/305)
  [#306](https://github.com/rust-lang/cargo-bisect-rustc/pull/306)
  [#308](https://github.com/rust-lang/cargo-bisect-rustc/pull/308)

## Fixed
- Fixed an issue when attempting to bisect a rollup, but the perf commits have been garbage collected, to display information about the rollup so that you can see which PRs were involved.
  [#298](https://github.com/rust-lang/cargo-bisect-rustc/pull/298)

## v0.6.7

### Changed
- Updated dependencies.
  [#271](https://github.com/rust-lang/cargo-bisect-rustc/pull/271)
  [#270](https://github.com/rust-lang/cargo-bisect-rustc/pull/270)
  [#273](https://github.com/rust-lang/cargo-bisect-rustc/pull/273)
  [#278](https://github.com/rust-lang/cargo-bisect-rustc/pull/278)
  [#279](https://github.com/rust-lang/cargo-bisect-rustc/pull/279)
  [#281](https://github.com/rust-lang/cargo-bisect-rustc/pull/281)
  [#285](https://github.com/rust-lang/cargo-bisect-rustc/pull/285)
- CI artifacts are now downloaded from https://ci-artifacts.rust-lang.org instead of https://s3-us-west-1.amazonaws.com/rust-lang-ci2 which should help with performance.

### Fixed
- Fix bisecting into rollups via unrolled perf builds
  [#280](https://github.com/rust-lang/cargo-bisect-rustc/pull/280)

## v0.6.6

### Added

- 🎉 Added bisecting of rollups. This depends on the artifacts generated for rustc-perf which is only available for x86_64-unknown-linux-gnu.
  [#256](https://github.com/rust-lang/cargo-bisect-rustc/pull/256)
- 🎉 Added a new User Guide with more detailed documentation and a set of examples illustrating different ways to use `cargo-bisect-rustc`. The guide is available at <https://rust-lang.github.io/cargo-bisect-rustc/>.
  [#266](https://github.com/rust-lang/cargo-bisect-rustc/pull/266)

### Changed

- Added another kind of ICE output that is auto-detected.
  [#261](https://github.com/rust-lang/cargo-bisect-rustc/pull/261)
- Updated dependencies:
  - tokio [#245](https://github.com/rust-lang/cargo-bisect-rustc/pull/245) [#255](https://github.com/rust-lang/cargo-bisect-rustc/pull/255)
  - git2 [#246](https://github.com/rust-lang/cargo-bisect-rustc/pull/246) [#249](https://github.com/rust-lang/cargo-bisect-rustc/pull/249)
  - bumpalo [#250](https://github.com/rust-lang/cargo-bisect-rustc/pull/250)
  - pbr [#257](https://github.com/rust-lang/cargo-bisect-rustc/pull/257)
  - tempfile [#260](https://github.com/rust-lang/cargo-bisect-rustc/pull/260)
  - openssl [#267](https://github.com/rust-lang/cargo-bisect-rustc/pull/267)
  - chrono [#268](https://github.com/rust-lang/cargo-bisect-rustc/pull/268)

### Fixed

- Fixed bounds checking when `--start` or `--end` is not specified.
  [#243](https://github.com/rust-lang/cargo-bisect-rustc/pull/243)
- The remote tags are now fetched from the `rust-lang/rust` repo to ensure that tag boundaries (`--start 1.65.0`) work if the tag hasn't been downloaded.
  [#263](https://github.com/rust-lang/cargo-bisect-rustc/pull/263)

## v0.6.5

### Changed

- Stack overflow on any thread (not just 'rustc') is treated as an ICE.
  [#194](https://github.com/rust-lang/cargo-bisect-rustc/pull/194)
- Clap (the CLI argument processor) has been updated, which may result in some minor CLI output and parsing changes.
  [#225](https://github.com/rust-lang/cargo-bisect-rustc/pull/225)
  [#229](https://github.com/rust-lang/cargo-bisect-rustc/pull/229)
- The check for the Rust upstream remote in the git repository has been loosened to only scan for `rust-lang/rust` so that non-https remotes like `git@github.com:rust-lang/rust.git` will work.
  [#235](https://github.com/rust-lang/cargo-bisect-rustc/pull/235)
- The `--script` option will now look for a script in the current directory (so that it no longer requires the `./` prefix).
  [#236](https://github.com/rust-lang/cargo-bisect-rustc/pull/236)
  [#238](https://github.com/rust-lang/cargo-bisect-rustc/pull/238)
- Specifying `--start` without `--end` will default the end to be the current date. Previously it would use the date of whatever nightly is currently installed.
  [#240](https://github.com/rust-lang/cargo-bisect-rustc/pull/240)

### Fixed

- Fixed using either `cargo bisect-rustc` (with a space) or `cargo-bisect-rustc` (with a dash).
  [#187](https://github.com/rust-lang/cargo-bisect-rustc/pull/187)
- Show the CLI help page if no arguments are passed.
  [#206](https://github.com/rust-lang/cargo-bisect-rustc/pull/206)
- The CLI argument validator for `--script` has been removed to allow running scripts on PATH. This also removes the `--host` validator which was not needed.
  [#207](https://github.com/rust-lang/cargo-bisect-rustc/pull/207)
- Fixed showing the full chain of errors instead of just the top-level one.
  [#237](https://github.com/rust-lang/cargo-bisect-rustc/pull/237)

## v0.6.4

### Added

- Added the `--component` option to choose optional components to install.
  [#131](https://github.com/rust-lang/cargo-bisect-rustc/pull/131)
- An estimate of the number of steps left to run is now displayed.
  [#178](https://github.com/rust-lang/cargo-bisect-rustc/pull/178)

### Changed

- Various code refactorings and dependency updates. These shouldn't have
  significant noticeable changes.
  [#151](https://github.com/rust-lang/cargo-bisect-rustc/pull/151)
  [#152](https://github.com/rust-lang/cargo-bisect-rustc/pull/152)
  [#153](https://github.com/rust-lang/cargo-bisect-rustc/pull/153)
  [#155](https://github.com/rust-lang/cargo-bisect-rustc/pull/155)
  [#156](https://github.com/rust-lang/cargo-bisect-rustc/pull/156)
- The `CARGO_BUILD_TARGET` environment variable is now set with the target triple.
  [#159](https://github.com/rust-lang/cargo-bisect-rustc/pull/159)
- The default release profile now uses stripping and LTO to significantly
  reduce the binary size.
  [#157](https://github.com/rust-lang/cargo-bisect-rustc/pull/157)
- Bounds with tags like `--start=1.62.0` are now translated to a nightly date
  instead of a master git commit. This allows using tags from releases more
  than 6 months old.
  [#177](https://github.com/rust-lang/cargo-bisect-rustc/pull/177)

## v0.6.3

### Fixed

- Fixed assumption that the rust-lang/rust remote repo is named "origin".
  [#149](https://github.com/rust-lang/cargo-bisect-rustc/pull/149)

## v0.6.2

### Added

- `--start` and `--end` now support git tags (like `1.59.0`) to bisect between stable releases.
  [#147](https://github.com/rust-lang/cargo-bisect-rustc/pull/147)

### Changed

- Stack overflow is now treated as an ICE.
  [#142](https://github.com/rust-lang/cargo-bisect-rustc/pull/142)

## v0.6.1

### Added

- Added `--with-dev` option to download rustc-dev component.
  [#101](https://github.com/rust-lang/cargo-bisect-rustc/pull/101)
- Added `--timeout` option to trigger a failure if compilation takes too long.
  [#135](https://github.com/rust-lang/cargo-bisect-rustc/pull/135)

### Changed
- Use the `git` CLI to fetch the `rust-lang/rust` repo when looking for CI commits to improve performance.
  [#130](https://github.com/rust-lang/cargo-bisect-rustc/pull/130)

### Fixed

- Fixed off-by-one error when examining the date of the local nightly toolchain.
  [#113](https://github.com/rust-lang/cargo-bisect-rustc/pull/113)
- Fixed issue with `--preserve` when linking the nightly toolchain leaving a stale link.
  [#125](https://github.com/rust-lang/cargo-bisect-rustc/pull/125)

## v0.6.0

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

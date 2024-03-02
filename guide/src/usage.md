# Basic usage

Using `cargo-bisect-rustc` simply involves running it inside a Cargo project that reproduces the regression:

```sh
cargo bisect-rustc
```

> For a quick introduction, see the [Tutorial](tutorial.md).

> **Note**: On Windows, due to [an issue](https://github.com/rust-lang/cargo-bisect-rustc/issues/244) with rustup, you will need to execute `cargo-bisect-rustc` with a `-` between `cargo` and `bisect`.

`cargo-bisect-rustc` works by building a Cargo project, and detecting if it succeeds or fails.
It will download and use nightly Rust toolchains.
It begins with two nightly boundaries, known as the *start* where the project successfully builds (the *baseline*), and the *end* where it is known to fail (the *regression*).
It will then do a binary search between those dates to find the nightly where the project started to fail.

Once it finds the nightly where it started to fail, `cargo-bisect-rustc` will then try to find the individual PR where it regressed.
The Rust project keeps the builds of every merged PR for the last 167 days.
If the nightly is within that range, then it will bisect between those PRs.

And even further, if the regression is in a [rollup PR], then it will bisect the individual PRs within the rollup.
This final bisection is only available for `x86_64-unknown-linux-gnu` since it is using the builds made for the [rustc performance tracker].

[rollup PR]: https://forge.rust-lang.org/release/rollups.html
[rustc performance tracker]: https://perf.rust-lang.org/

## Rust src repo

`cargo-bisect-rustc` needs to read the git log of the [`rust-lang/rust`] repo in order to scan individual commits.
See the [Rust src repo] chapter for details on how to configure how it finds the git repo.

[Rust src repo]: rust-src-repo.md
[`rust-lang/rust`]: https://github.com/rust-lang/rust/

## Boundaries

Without setting any options, `cargo-bisect-rustc` will try to automatically find the *start* where the build succeeds and the *end* where it fails.
This can take some time, depending on how far back it needs to scan.
It is recommended to use the `--start` and `--end` CLI options to tell it where the boundaries are.

```sh
cargo bisect-rustc --start=2022-11-01 --end=2023-02-14
```

See the [Bisection boundaries] chapter for more details on setting these options.

[Bisection boundaries]: boundaries.md

## Regression check

By default, `cargo-bisect-rustc` assumes the *start* boundary successfully builds, and the *end* boundary fails to build.
You can change this using the `--regress` CLI option.
For example, you can tell it that the *start* should fail, and the *end* should pass.
There are several options you can use with the `--regress` flag:

<style>
    table td:nth-child(1) {
        white-space: nowrap;
    }
</style>

| Option | Start | End | Description |
|--------|-------|-----|-------------|
| `error` | Succeed | Fail | The default setting checks for a failure as the regression. |
| `success` | Fail | Succeed | Reverses the check to find when something is *fixed*. |
| `ice` | No ICE | ICE | Scans when an Internal Compiler Error (ICE) was introduced. |
| `non-ice` | ICE | No ICE | Scans when an ICE was fixed. |
| `non-error` | Non-ICE Failure | Succeed or ICE | Scans when an ill-formed program stops being properly rejected, or the compiler starts generating an ICE. |

See [Scripting](#scripting) for customizing this behavior.

## Custom commands

By default, `cargo-bisect-rustc` runs `cargo build`.
You can change which `cargo` command is run by passing additional arguments after `--`:

```sh
cargo bisect rustc -- test --test mytest
```

## Scripting

You can use an arbitrary script for determining what is a baseline and regression.
This is an extremely flexible option that allows you to perform any action automatically.
Just pass the path to the script to the `--script` CLI command:

```sh
cargo bisect-rustc --script ./test.sh
```

The script should exit 0 for the baseline, and nonzero for a regression.
Since `cargo-bisect-rustc` sets `RUSTUP_TOOLCHAIN` (see [Rustup toolchains](rustup.md)), all you need to do is call `cargo` or `rustc`, and the script should automatically use the toolchain that is currently being tested.

```sh
#!/bin/sh

set -ex

# This checks that a warning is only printed once.
# See https://github.com/rust-lang/rust/issues/88256 for a regression where it
# started printing twice.

OUTPUT=`cargo check 2>&1`
COUNT=`echo "$OUTPUT" | grep -c "unnecessary parentheses"`
test $COUNT -eq 1
```

If you need to use the targets directly without using `cargo` in the script, they are available in `$CARGO_TARGET_DIR/[release|debug]/...`, since `cargo-bisect-rustc` sets `$CARGO_TARGET_DIR`.

Check out the [examples chapters](examples/index.md) for several examples of how to use this option.

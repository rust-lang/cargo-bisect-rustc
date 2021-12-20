# Bisecting Regressions

The [`cargo-bisect-rustc`] tool makes it super easy to find exactly when
behavior has regressed in rustc. It automatically downloads rustc
artifacts and tests them against a project you provide until it finds
the regression.

## Installation

If you're going to bisect for linux-musl host, install musl toolchain and run:

```sh
cargo install cargo-bisect-rustc --features git2/vendored-openssl
```

Otherwise, run:

```sh
cargo install cargo-bisect-rustc
```

or (to avoid cloning rustc)

```
RUST_SRC_REPO=/path/to/rust cargo install cargo-bisect-rustc
```

The `RUST_SRC_REPO` should be a path to a git clone of the rust repo.
The current order how `cargo-bisect-rustc` finds Rust repository path is:
* `RUST_SRC_REPO` at runtime.
* `rust.git` in current dir at runtime.
* `RUST_SRC_REPO` that was set at compilation time.
* Clone https://github.com/rust-lang/rust automatically
  (only necessary if doing git hash bisections).

First, if you have a nightly version of the compiler already installed
as the default toolchain and you don't pass an end flag, the tool is
going to assume that that's the version that has regressed and use it as
the "bad" version. Otherwise would use that start point or just use the
latest nightly.
If you have provided a start flag it would use that as the "good"
version, otherwise is going to search for a good one backwards.

Once the tool has an start point (good version) and end point (bad
version), is going to do the bisect to find the regressed nightly. Once
it finds the regressed nightly is going to look between that nightly and
the one of the day before for the specific commit that has the
regression and report everything back to the user.

So, there's a bunch of ways to run the tool, the easiest one is to
allow the tool to do the job and just run `cargo bisect-rustc` on the
project and let the tool figure things out.

## Finding a regression

Create a cargo project that demonstrates the regression. Let's use
[issue #53157] as an example:

```
cargo new foo
```

Edit `foo/src/main.rs` with the example from the issue:

```rust
macro_rules! m {
    () => {{
        fn f(_: impl Sized) {}
        f
    }}
}

fn main() {
    fn f() -> impl Sized {};
    m!()(f());
}
```

Then run `cargo bisect-rustc --end=2018-08-04`.

We need to provide the end point for this particular example because
that's an old regression already fixed on the latests nightlies.
We could also provide a start point if we know one, that's going to make
the tool avoid searching for that so answer our request faster.
For instance:

```
cargo bisect-rustc --test-dir=foo --start=2018-05-07 --end=2018-08-04
```

By default it will run `cargo build` in the project and check whether or not
it fails.  You can also use the flag `--regress` to specify other common
regression criteria, e.g. `--regress=ice` for internal compiler errors.

In out example, in just a few steps, we can we find that it stopped working on
`nightly-2018-07-30`.

> *Note:* Consider using the `--preserve` flag to keep the downloaded
> artifacts for future runs. They are stored in the normal location for your
> toolchains in `RUSTUP_HOME`.

After that is going to automatically search for the commit that
introduced the regression.

## Finding a regression between commits

We can also just ask the tool to look between commits if that's what we
want. As long as the regression wasn't too long ago, we can find the
exact PR that caused the regression. Use git hashes from the rustc
repo's log as the start/end parameters. They must be from bors on the
master branch.

To find a list of all such usable commit hashes, we can use `git log` in the
`RUST_SRC_REPO` git clone. After regressing to a nightly, and padding a couple
days before and after its date to allow for the CI build process time:

```
git log --since "JUL 28 2018" --until "JUL 30 2018" --author=bors --pretty=format:"%H %an %ad"
```

will show

```
e4378412ecfc2a4ff5dfd65fef53fa6be691f689 bors Mon Jul 30 10:19:38 2018 +0000
5ed2b5120bd875a7eb9fd8545d86eb1de1e41bce bors Mon Jul 30 08:25:36 2018 +0000
7bbcd005b30582d07f1a39dcf50f77b54e055828 bors Mon Jul 30 06:29:39 2018 +0000
a3f519df09bf40d09c1a111599b8f115f11fbb49 bors Mon Jul 30 04:34:19 2018 +0000
b12235db096ab24a31e6e894757abfe8b018d44a bors Mon Jul 30 01:08:13 2018 +0000
866a713258915e6cbb212d135f751a6a8c9e1c0a bors Sun Jul 29 21:37:47 2018 +0000
70cac59031d5c33962a1f53cdca9359c0dcd1f9f bors Sun Jul 29 19:37:28 2018 +0000
75af9df71b9eea84f281cf7de72c3e3cc2b02222 bors Sun Jul 29 13:23:01 2018 +0000
2a9dc245c60ab4478b3bc4670aaad4b39e646366 bors Sun Jul 29 11:27:48 2018 +0000
023fd7e74a9eb5bafcb75fcbe69b7110e9de4492 bors Sun Jul 29 09:33:37 2018 +0000
a5c2d0fffaaf0b764c01bc4066e51ffd475ceae9 bors Sun Jul 29 06:32:24 2018 +0000
fb0653e40289eecf32f3fac1e84fc69b815ce5cb bors Sun Jul 29 03:20:54 2018 +0000
6a2c97c38d297307dd8554853890f51144f62172 bors Sun Jul 29 01:14:39 2018 +0000
6323d9a45bdf0ac2a9319a6a558537e0a7e6abd1 bors Sat Jul 28 23:10:10 2018 +0000
dab71516f1f4f6a63e32dffeb2625a12e5113485 bors Sat Jul 28 20:44:17 2018 +0000
4234adf0d4fa56e8a8b8d790fb4992d160ab2188 bors Sat Jul 28 18:41:40 2018 +0000
d75458200516f06455d175adc001fd993d674050 bors Sat Jul 28 16:44:21 2018 +0000
26e73dabeb7a15e0e38feb2cadca3c1f740a61d2 bors Sat Jul 28 14:26:16 2018 +0000
5b465e309da475aaedcb742ef29094c82e970051 bors Sat Jul 28 11:37:41 2018 +0000
```

and we can, for example, pick the last commit on the day before the nightly,
`6323d9a45bdf0ac2a9319a6a558537e0a7e6abd1`, as the start of the range, and the
last commit on the day of the nightly, `866a713258915e6cbb212d135f751a6a8c9e1c0a`,
as the end of the range.

Assuming you aren't reading this too far in the future, the
following should work:

```
cargo-bisect-rustc --test-dir=foo \
    --start=6323d9a45bdf0ac2a9319a6a558537e0a7e6abd1 \
    --end=866a713258915e6cbb212d135f751a6a8c9e1c0a
```

This tells us that the regression started with
`70cac59031d5c33962a1f53cdca9359c0dcd1f9f` and you can look at the git log to
find the PR. Here, #51361.

```
 git log -1 70cac59031d5c33962a1f53cdca9359c0dcd1f9f
```

shows the merge commit's description starts with "`Auto merge of #51361`".

## Testing interactively

Pass/fail of `cargo build` may not be what you're after. Perhaps the issue is
an error message changed, so both the "good" and "bad" version will fail to
compile, just with a different message. Or maybe something used to fail, and
now erroneously passes. You can use the interactive feature with the
`--prompt` flag to visually inspect a build and tell `cargo-bisect-rustc`
what's "good" and what's "bad". Let's use [issue #55036] as an example where
an error message changed:

`foo/src/main.rs`:
```rust
struct Foo {
    bar: i32
}

trait Baz {
    fn f(Foo { bar }: Foo) {}
}
```

This historically emitted a bad error, was updated to emit a nice error (E0642
added in #53051), but then that nice error was lost somewhere (on the 2015
edition). Let's find where it was lost! Grab the ranges between where it was
added and where we know it fails:

```
cargo-bisect-rustc --prompt --test-dir=foo \
    --start=ab93561b5fa54954159480ddc10bbb69f015e539 \
    --end=2c2e2c57dc2140cfb62a8abb9312b89f02c59f3c
```

At each step, `cargo-bisect-rustc` will show the output and ask you:

```
ab93561b5fa54954159480ddc10bbb69f015e539 finished with exit code Some(101).
please select an action to take:
> mark regressed
  mark baseline
  retry
```

Choose `mark baseline` with the nice E0642 message, and `mark regressed` with
the less-favorable token error. Fairly quickly we find it regressed in
af50e3822c4ceda60445c4a2adbb3bfa480ebd39 which is a rollup merge. However,
it's not too hard to look through the commits and find a likely culprit.

## Testing with a script

Using the `--script` option allows you to do something more fancy than just
`cargo build`. Maybe you need to run cargo multiple times, or just call
`rustc` directly, or you want to automatically grep through the output. The
possibilities are endless! Just write a little shell script that exits 0 for
the baseline, and exits nonzero for the regression. As an example, the
previous interactive session can be hands-free automated with this script:

`foo/test.sh`:
```sh
#!/bin/sh

# Fail if we no longer get a `E0642` error:
cargo check 2>&1 | grep E0642
```

And then run:

```
cargo-bisect-rustc --script=./test.sh --test-dir=foo \
    --start=ab93561b5fa54954159480ddc10bbb69f015e539 \
    --end=2c2e2c57dc2140cfb62a8abb9312b89f02c59f3c
```

## Varying tests

When writing your test and picking a bisection range, you should be careful to
ensure that the test won't vary between pass/fail over time. It should only
transition from good to bad once in the bisection range (it must change
[monotonically]). The following are some suggestions for dealing with a
potentially varying test:

* Use the `-vv` flag (very verbose) to display the output from the compiler to
  make sure it is what you expect.
* Use the [`--prompt`](#testing-interactively) flag to inspect the output and
  verify each step.
* Beware that some issues may get fixed and then regress multiple times. Try
  to keep the bisection range as close to the present day as possible. Compare
  the output of the "regressed" commit to the latest nightly to see if they
  are the same.
* If the test only fails sporadically, use a [script](#testing-with-a-script)
  to run the compiler many times until it fails, or it passes enough
  iterations that you feel confident that it is good.
* If the code requires relatively new language features, be careful not to
  pick a starting range that is too old.

[monotonically]: https://en.wikipedia.org/wiki/Bisection_(software_engineering)#Monotonicity
[`cargo-bisect-rustc`]: https://github.com/rust-lang/cargo-bisect-rustc
[issue #53157]: https://github.com/rust-lang/rust/issues/53157
[issue #55036]: https://github.com/rust-lang/rust/issues/55036

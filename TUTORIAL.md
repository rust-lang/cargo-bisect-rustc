# Bisecting Regressions

The [`cargo-bisect-rustc`] tool makes it super easy to find exactly when
behavior has changed in rustc. It automatically downloads rustc artifacts and
tests them against a project you provide until it finds the regression. To
install:

```sh
git clone https://github.com/rust-lang-nursery/cargo-bisect-rustc.git
RUST_SRC_REPO=/path/to/rust cargo install --path=cargo-bisect-rustc
```

The `RUST_SRC_REPO` should be a path to a git clone of the rust repo. If you
don't specify it, it will look in the current directory for `rust.git` or
check it out automatically if it's not there (only necessary if doing git hash
bisections).

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

## Regressing to a nightly

Let's find the first nightly where this fails. First you need to determine a
nightly release where the code works, and one where it doesn't. We can see
from the issue that we know stable 1.28.0 was working, and that it no longer
works as of nightly 2018-08-04. We're testing against nightlies, and stable
1.28.0 branched off master 12 weeks prior to its release, and the regression
could have been introduced to master during that time, so I'll just take a
guess that nightly 2018-05-07 is OK. Don't worry if you guess wrong,
`cargo-bisect-rustc` will tell you and you can just pick a larger date range.
To find the exact nightly release where it stopped working:

```
cargo-bisect-rustc --test-dir=foo --start=2018-05-07 --end=2018-08-04
```

By default it will run `cargo build` in the project and check whether or not
it fails. In just a few steps, we find that it stopped working on
`nightly-2018-07-30`.

> *Note:* Consider using the `--preserve` flag to keep the downloaded
> artifacts for future runs. They are stored in the normal location for your
> toolchains in `RUSTUP_HOME`.

## Regressing to a PR

But wait, we can do better! As long as the regression wasn't too long ago, we
can find the exact PR that caused the regression. Use git hashes from the
rustc repo's log as the start/end parameters. They must be from bors on the
master branch. Assuming you aren't reading this too far in the future, the
following should work:

```
cargo-bisect-rustc --test-dir=foo \
    --start=6323d9a45bdf0ac2a9319a6a558537e0a7e6abd1 \
    --end=866a713258915e6cbb212d135f751a6a8c9e1c0a
```

This tells us that the regression started with
70cac59031d5c33962a1f53cdca9359c0dcd1f9f and you can look at the git log to
find the PR.

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
cargo check 2>&1 | grep E0642
```

And then run:

```
cargo-bisect-rustc --script=./test.sh --test-dir=foo \
    --start=ab93561b5fa54954159480ddc10bbb69f015e539 \
    --end=2c2e2c57dc2140cfb62a8abb9312b89f02c59f3c
```


[`cargo-bisect-rustc`]: https://github.com/rust-lang-nursery/cargo-bisect-rustc
[issue #53157]: https://github.com/rust-lang/rust/issues/53157
[issue #55036]: https://github.com/rust-lang/rust/issues/55036

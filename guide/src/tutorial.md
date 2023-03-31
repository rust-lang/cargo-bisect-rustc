# Tutorial

`cargo-bisect-rustc` works by building a Cargo project and checking if it succeeds or fails.
This tutorial walks through an example of this process.

## Finding a regression

Create a cargo project that demonstrates the regression.
Let's use [issue #53157] as an example:

```sh
cargo new foo
cd foo
```

Edit `src/main.rs` with the example from the issue:

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

Since we are testing an old regression, also edit `Cargo.toml` to remove the `edition = "2021"` field which isn't supported in these versions.

Then run `cargo bisect-rustc --end=2018-08-04`.

We need to provide the end point for this particular example because that's an old regression already fixed on the latest nightlies.
We could also provide a start point if we know one;
that's going to make it faster by avoiding scanning for the start.
For instance:

```sh
cargo bisect-rustc --start=2018-05-07 --end=2018-08-04
```

It will run `cargo build` in the project and check whether or not it fails.
It will do a binary search between the start and end range to find exactly where the regression occurred.

> **Note**: You can also use the flag [`--regress`] to specify other common regression criteria, e.g. `--regress=ice` for internal compiler errors.

[`--regress`]: usage.md#regression-check

In our example, in just a few steps, we can we find that it stopped working on `nightly-2018-07-30`.

If the regression is recent enough, then it will print out a list of PRs that were committed on that date.
In this particular example, it is too old, so we'll need to manually inspect the git log to see which PR's were merged.

If the nightly was within the last 167 days, then `cargo-bisect-rustc` will then start bisecting those individual PRs.

After finding potential candidates, you can go inspect those PRs to see which one is the likely cause.
In this case, since the ICE was in MIR const propagation, and #51361 is the likely candidate since it modified const evaluation.

## Testing interactively

Pass/fail of `cargo build` may not be what you're after.
Perhaps the issue is an error message changed, so both the "good" and "bad" version will fail to
compile, just with a different message.
Or maybe something used to fail, and now erroneously passes.
You can use the interactive feature with the `--prompt` flag to visually inspect a build and tell `cargo-bisect-rustc` what's "good" and what's "bad".
Let's use [issue #55036] as an example where an error message changed:

In `Cargo.toml`, remove the `edition` field (this example was before editions).

`src/main.rs`:
```rust
struct Foo {
    bar: i32
}

trait Baz {
    fn f(Foo { bar }: Foo) {}
}

fn main() {}
```

This historically emitted a bad error, was updated to emit a nice error (E0642 added in #53051), but then that nice error was lost somewhere (on the 2015 edition).
Let's find where it was lost!
Grab the ranges between where it was added and where we know it fails:

```sh
cargo bisect-rustc --prompt \
    --start=2018-08-14 \
    --end=2018-10-11
```

At each step, `cargo-bisect-rustc` will show the output and ask you:

```text
nightly-2018-08-14 finished with exit code Some(101).
please select an action to take:
> mark regressed
  mark baseline
  retry
```

Choose `mark baseline` with the nice E0642 message, and `mark regressed` with the less-favorable token error.
Fairly quickly we find it regressed in nightly-2018-10-11.
The most likely candidate is #54457 which is a rollup PR.
It's usually not too hard to look through the commits and find a likely culprit.
Indeed in this example, #54415 modified function parameter parsing.

## Testing with a script

Using the `--script` option allows you to do something more fancy than just `cargo build`.
Maybe you need to run cargo multiple times, or just call `rustc` directly, or you want to automatically grep through the output.
The possibilities are endless!
Just write a little shell script that exits 0 for the baseline, and exits nonzero for the regression.
As an example, the previous interactive session can be hands-free automated with this script:

`test.sh`:
```sh
#!/bin/sh

# Fail if we no longer get a `E0642` error:
cargo check 2>&1 | grep E0642
```

And then run:

```sh
cargo bisect-rustc --script=./test.sh \
    --start=2018-08-14 \
    --end=2018-10-11
```

[issue #53157]: https://github.com/rust-lang/rust/issues/53157
[issue #55036]: https://github.com/rust-lang/rust/issues/55036

# Bisection boundaries

`cargo-bisect-rustc` does a binary search for the regression using a *start* and *end* boundary.
You can specify these boundaries with the `--start` and `--end` CLI flags.
There are several ways to specify what those boundaries are.
If you run the command without specifying the boundaries, it will search for them automatically:

```sh
# No --start or --end flags
cargo bisect-rustc
```

This will assume the latest nightly is a regression (the *end* boundary).
It will then search backwards until it can find a nightly that passes to use as the *start* boundary.
Bisection can usually go faster if you happen to know the start boundary, so that it doesn't need to search for it.

`--start` and `--end` are optional.
If `--start` is not specified, then it will try to find the start range automatically.
If `--end` is not specified, it will assume it is the most recently available.

## Date boundaries

You can pass a date in the form YYYY-MM-DD to the `--start` and `--end` flags.
It will download the nightly corresponding to that date, and then begin bisecting those nightlies.

```sh
cargo bisect-rustc --start=2018-08-14 --end=2018-10-11
```

If the nightly with the regression was within the past 167 days, then it will automatically start bisecting the individual PRs merged on that day using [Git commit boundaries](#git-commit-boundaries).

## Git commit boundaries

You can pass the particular git commit hash of a PR as a boundary.
The Rust project keeps the builds of every merged PR for the last 167 days.
If you happen to know the PR to use as a boundary, you can pass the SHA-1 hash of that PR.

```sh
cargo bisect-rustc \
    --start=6323d9a45bdf0ac2a9319a6a558537e0a7e6abd1 \
    --end=866a713258915e6cbb212d135f751a6a8c9e1c0a
```

There are several ways to determine the SHA-1 hash for a PR.

- On the PR itself, you should see a message like "bors merged commit c50c62d into `rust-lang:master`".
  You can copy that hash to use as a boundary.
  If the PR was merged as part of a rollup, you will need to use the hash of the rollup instead.
  You'll need to look through the PR messages to see if the PR was mentioned from a rollup PR.
- In the rust repo, run `git log --first-parent upstream/master` (where `upstream` is your origin name for `rust-lang/rust`).
  This will show all the top-level commits.
  You can then search for your PR.

> **Note**: If the PR was merged after the most recent nightly, you'll need to be sure to also specify the `--end` range.
> Otherwise it will assume the most recent nightly is the *end* and it won't work if the start is after the end.

If the regression is found in a [rollup PR], then `cargo-bisect-rustc` will bisect the individual PRs within the rollup.
This final bisection is only available for `x86_64-unknown-linux-gnu` since it is using the builds made for the [rustc performance tracker].

> **Note**: If you specify date boundaries, then you can use the `--by-commit` CLI option to force it to use PR commits instead of nightlies.

[rollup PR]: https://forge.rust-lang.org/release/rollups.html
[rustc performance tracker]: https://perf.rust-lang.org/

## Git tag boundaries

The boundary can be specified with a git release tag.
This is useful if you know something works in one release and not another, but you don't happen to know which nightly this corresponds with.
When given a tag, `cargo-bisect-rustc` will try to find the nightly that corresponds with that release.
For example:

```sh
cargo bisect-rustc --start=1.58.0 --end=1.59.0
```

## Monotonicity

When writing your test and picking a bisection range, you should be careful to ensure that the test won't vary between pass/fail over the bisection range.
It should only transition from good to bad once in the bisection range (it must change
[monotonically]).

In the following example, `cargo-bisect-rustc` will find one of the transitions, but that may not be the true root cause of the issue you are investigating.

```text
nightly-2023-02-01 baseline **start**
nightly-2023-02-02 baseline
nightly-2023-02-03 baseline
nightly-2023-02-04 regression
nightly-2023-02-05 regression
nightly-2023-02-06 baseline
nightly-2023-02-07 regression
nightly-2023-02-08 regression **end**
```

Here it may either find 2023-02-04 or 2023-02-07 as the regression.

The following are some suggestions for avoiding or dealing with this problem:

- Make sure your test reliably exhibits the issue you are looking for, and does not generate any false positives or false negatives.
- Analyze the PR that was reported as the regression.
  Do the changes in the PR seem to be a probable cause?
- Try to keep the bisection range small to reduce the probability that you will encounter multiple regression transitions.
- Use the `-vv` flag (very verbose) to display the output from the compiler to make sure it is what you expect.
- Use the [`--prompt`](tutorial.md#testing-interactively) flag to inspect the output and verify each step.
- Beware that some issues may get fixed and then regress multiple times.
  Try to keep the bisection range as close to the present day as possible.
  Compare the output of the "regressed" commit to the latest nightly to see if they are the same.
- If the test only fails sporadically, use a [script](examples/flaky.md) to run the compiler many times until it fails or it passes enough iterations that you feel confident that it is good.
- If the code requires relatively new language features, be careful not to pick a starting range that is too old.
- Beware of code-generation bugs that can be sensitive to code layout.
  Since the code to rustc changes rapidly over time, code can shift around causing different layouts and optimizations, which might cause an issue to appear and disappear several times over the bisection range.

[monotonically]: https://en.wikipedia.org/wiki/Bisection_(software_engineering)#Monotonicity

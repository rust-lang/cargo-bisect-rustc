# Flaky errors

Some tests may fail randomly.
The following script is an example that will run `rustc` repeatedly to check for a failure.
This example is from [#108216](https://github.com/rust-lang/rust/issues/108216) (which requires macOS).

`test.sh`:
```sh
#!/bin/sh

rm -rf *.o incremental foo

echo "fn main() { let a: i64 = 1 << 64; }" > foo1.rs
echo "fn main() { let a: i64 = 1 << 63; }" > foo2.rs

ARGS="--crate-name foo -C split-debuginfo=unpacked -C debuginfo=2 -C incremental=incremental"

for i in {1..20}
do
    echo run $i
    rustc foo1.rs $ARGS && { echo "ERROR: first build should have failed"; exit 1; }
    rustc foo2.rs $ARGS || { echo "ERROR: second build should have passed"; exit 1; }
    ./foo || { echo "ERROR: executing should have passed"; exit 1; }
done
```

This test can be run with:

```sh
cargo bisect-rustc --start=1.57.0 --end=1.58.0 --script=./test.sh
```

In general, configure the script to perform whichever actions you need in a `for` loop that runs enough times that you have a high confidence it has found the regression.

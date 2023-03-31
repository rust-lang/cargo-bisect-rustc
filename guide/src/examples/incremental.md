# Incremental compilation

Testing for regressions with incremental compilation may require running a command multiple times.
The following illustrates an example for [#87384](https://github.com/rust-lang/rust/issues/87384) which only generates a warning the second time a build is run with incremental.
Previously no warning was emitted.

`foo.rs`:
```rust
#![type_length_limit = "95595489"]

pub fn main() {
    println!("Hello, world!");
}
```

Create a script `test.sh`:

```sh
#!/bin/sh

# Exit if any command fails.
set -e

rm -rf incremental
rustc foo.rs --crate-type lib -C incremental=incremental
echo second
OUTPUT=`rustc foo.rs --crate-type lib -C incremental=incremental 2>&1`
echo $OUTPUT
! echo "$OUTPUT" | grep \
    "crate-level attribute should be in the root module"
```

Run this script with:

```sh
cargo-bisect-rustc --start 1.54.0 --end 1.55.0 --script ./test.sh
```

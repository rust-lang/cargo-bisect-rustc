# Bisecting Rustdoc

`cargo-bisect-rustc` can be used to check for Rustdoc regressions, too.
All you need to do is instruct it to use the correct command.

The following example will check to find a regression when `cargo doc` suddenly starts to fail.

```sh
cargo bisect-rustc --start=2022-08-05 --end=2022-09-09 -- doc
```

Some rustdoc regressions might be in the generated HTML output.
To scan the output, you can use a script like the following:

`test.sh`:
```sh
#!/bin/sh

# Exit if any command fails.
set -e

cargo doc

grep "some example text" $CARGO_TARGET_DIR/doc/mycrate/fn.foo.html
```

This can be used with the `--script` option:

```sh
cargo-bisect-rustc --start=2023-01-22 --end=2023-03-18 --script=./test.sh \
    --term-old="Found example text" --term-new="Failed, or did not find text"
```

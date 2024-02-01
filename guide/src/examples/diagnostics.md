# Checking diagnostics

The following is an example of checking when the diagnostic output of `rustc` *changes*.
For example, this can check when either the wording has changed, or a different error or warning is produced.

[#109067](https://github.com/rust-lang/rust/issues/109067) is an example of where this is necessary.
A warning started being emitted, and it is the kind of warning that cannot be turned into an error with `deny(warnings)`.

The following script is intended to be used with the `--script` option (set the executable flag on the script, `chmod u+x`):

```sh
#!/bin/sh

OUTPUT=`cargo check 2>&1`
# Comment out this test if your example is intended to fail.
if [ $? -ne 0 ]
then
    echo "Build unexpectedly failed: $OUTPUT"
    exit 1
fi
# Display the output for debugging purposes.
# Run `cargo-bisect-rustc` with `-vv` to view the output.
echo "$OUTPUT"
# This indicates a regression when the text "non-ASCII" is in the output.
#
# If the regression is when the text is *not* in the output, remove the `!` prefix.
! echo "$OUTPUT" | grep "non-ASCII"
```

Then run something like:

```sh
cargo bisect-rustc --start=1.67.0 --end=1.68.0 --script ./test.sh
```

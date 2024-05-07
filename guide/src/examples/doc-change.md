# Documentation changes

`cargo-bisect-rustc` can be used to scan for changes in the documentation shipped with each release.
This includes all the books and standard library documentation.
To do this, instruct it to download the component, and use a script that scans for whatever you are looking for.
You can use `rustup doc --path` or `rustc --print=sysroot` to find the proper location.
For example:

`test.sh`:
```sh
#!/bin/sh

# Exit if any command fails.
set -e

STD=`dirname $(rustup doc --std --path)`

# Checks if a particular file exists.
# This could also be `grep` or any other kinds of tests you need.
if [ -e $STD/io/error/type.RawOsError.html ]
then
    echo "found"
    exit 1
fi
```

And run with:

```sh
cargo bisect-rustc --start 1.68.0 --end 1.69.0 -c rust-docs --script ./test.sh
```

> **Note**: This may not work on all targets since `cargo-bisect-rustc` doesn't properly handle rustup manifests, which alias some targets to other targets.
> Use `--host x86_64-unknown-linux-gnu` in that situation.

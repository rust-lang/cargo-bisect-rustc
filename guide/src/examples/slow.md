# Slow or hung compilation

Some regressions may involve the compiler hanging or taking an unusually long time to run.
The `--timeout` CLI option can be used to check for this.
Let's use [#89524](https://github.com/rust-lang/rust/issues/89524) as an example.
A particular combination of factors caused the compiler to start to hang.

Change `Cargo.toml` to the following:

```toml
[package]
name = "slow"
version = "0.1.0"

[dependencies]
config = "=0.9.3"

[profile.release]
panic = "abort"
codegen-units = 1
```

Then use the timeout option:

```sh
cargo-bisect-rustc --start=2021-09-01 --end=2021-10-02 --timeout 30 -- build --release
```

You may need to adjust the timeout value based on the speed of your system.

> **Note**: `--timeout` is currently not working on macOS. See <https://github.com/rust-lang/cargo-bisect-rustc/issues/232>.

In some cases bisecting if a timeout happens is not enough, there might also be a compilation error (see for example [rustc#139197](https://github.com/rust-lang/rust/issues/139197)). In this case the script should handle all the work, here's a Bash example (given `main.rs` is the Rust code to reproduce the hanging compilation):
```sh
#!/bin/bash
res=$( timeout 3 rustc main.rs )
if [ "$?" -eq 124 ]; then
    # Excessive compile time
    exit 1
else
    # Compilation fails as expected *but* it doesn't hang
    exit 0
fi
```

and then run (example):
```sh
cargo-bisect-rustc [...params...] --script test.sh
```

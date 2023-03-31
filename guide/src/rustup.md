# Rustup toolchains

`cargo-bisect-rustc` takes advantage of [rustup toolchains] for installation and selecting the correct `rustc` to run.
It will essentially run `cargo +bisector-nightly-2023-03-18-x86_64-unknown-linux-gnu build` using rustup [toolchain override shorthand] to run the toolchains that it downloads.
This sets the `RUSTUP_TOOLCHAIN` environment variable to the toolchain name, which ensures that any call to `rustc` will use the correct toolchain.

By default, `cargo-bisect-rustc` will delete toolchains immediately after using them.
You can use the `--preserve` option to keep the toolchains so that you can use them manually.
See the [Preserving toolchains] example for more details.

When using the `--script` option, the script should just invoke `cargo` or `rustc` normally, and rely on the `RUSTUP_TOOLCHAIN` environment variable to pick the correct toolchain.

[rustup toolchains]: https://rust-lang.github.io/rustup/concepts/toolchains.html
[toolchain override shorthand]: https://rust-lang.github.io/rustup/overrides.html#toolchain-override-shorthand
[Preserving toolchains]: examples/preserve.md

# Using extra components

By default, `cargo-bisect-rustc` only fetches `rustc`, `cargo`, `rustdoc`, and the standard library for the host.
You may need additional [Rustup Components](https://rust-lang.github.io/rustup/concepts/components.html) to run your test.
Some examples of when this might be needed are:

* You want to find a regression in Clippy (see [Bisecting Clippy](clippy.md)), or miri.
* Scanning for when some documentation changed (see [Documentation changes](doc-change.md)).
* The platform needs additional things.
  For example, bisecting `x86_64-pc-windows-gnu` host may need the `rust-mingw` component.

If you are testing cross-compilation, use the `--target` option to download the standard library for the target you are using.

The following example shows how to use components to do a bisection with Cargo's [build-std](https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#build-std) feature.

```sh
cargo-bisect-rustc --start=2022-11-01 --end=2022-11-20 -c rust-src -- build -Zbuild-std
```

> **Note**: The `--with-src` option is an alias for `-c rust-src`. \
> The `--with-dev` option is an alias for `-c rustc-dev -c llvm-tools`.

# Running without cargo

Some bisections don't require Cargo.
You can use the `--without-cargo` option to skip installing cargo which can speed up the bisection since it doesn't need to download cargo, and doesn't have the overhead of running cargo.
You will need to pair this with `--script` since `cargo-bisect-rustc` assumes projects use Cargo.

For example, using a simple `rustc` command:

```sh
cargo-bisect-rustc --start=2022-11-01 --end=2022-11-20 --without-cargo --script=rustc -- foo.rs
```

> **Note**: You can use `--without-cargo` while still using a Cargo project.
> Rustup will fall back to using `cargo` from your installed nightly, beta, or stable toolchain.
> However, this isn't recommended since `cargo` is only intended to work with the version it is released with, and can sometimes be incompatible with different versions.
> But if you are bisecting a very recent change, then you can probably get away with it.

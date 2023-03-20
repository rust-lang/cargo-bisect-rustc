# Preserving toolchains

You may want to reuse the toolchains downloaded by `cargo-bisect-rustc` for doing further analysis or debugging.
Or, while setting up your regression test, you may need to adjust your test and script several times, and downloading the same toolchains multiple times can be quite slow.

You can do this with the `--preserve` option.

```sh
cargo bisect-rustc --start=2023-01-01 --end=2023-02-01 --preserve
```

The toolchains will be kept in your Rustup home directory (typically `~/.rustup/toolchains`).

Toolchains for nightlies will have the form of `bisector-nightly-YYYY-MM-DD-<target>`.
Toolchains for PR artifacts will have the form of `bisector-ci-<hash>-<target>`.

You can run these toolchains using a Rustup override, like this:

```sh
cargo +bisector-nightly-2023-03-18-x86_64-unknown-linux-gnu build
# or...
cargo +bisector-ci-e187f8871e3d553181c9d2d4ac111197a139ca0d-x86_64-unknown-linux-gnu build
```

When you are done, you'll probably want to clean up these directories since they use a lot of space.
The easiest method is to just delete the directories:

```sh
rm -rf ~/.rustup/toolchains/bisector-*
```

## Manually installing

The `--install` option can be used to only install a toolchain.
This won't do a bisection, it is just for fetching a toolchain for testing.

```sh
cargo bisect-rustc --install e187f8871e3d553181c9d2d4ac111197a139ca0d
```

> **Note**: See also [`rustup-toolchain-install-master`](https://github.com/kennytm/rustup-toolchain-install-master) which is specialized for installing CI artifacts.

# Installation

The basic method for installing `cargo-bisect-rustc` is:

```sh
cargo install cargo-bisect-rustc
```

Additional options are described below.

## Requirements

Besides having a working Rust installation, you may need a few other things installed on your system:

- Unix:
    - pkg-config
    - OpenSSL (`libssl-dev` on Ubuntu, `openssl-devel` on Fedora or Alpine)
- macOS:
    - OpenSSL ([homebrew] is recommended to install the `openssl` package)
- [rustup]

[homebrew]: https://brew.sh/
[rustup]: https://rustup.rs/

If you're having trouble using the system OpenSSL installation, it can be built from scratch.
The following will enable the vendored OpenSSL build:

```sh
cargo install cargo-bisect-rustc --features git2/vendored-openssl
```

Beware that this also requires `perl` and `make` to be installed.

## `RUST_SRC_REPO`

`cargo-bisect-rustc` needs to access the git log of the rust repo.
You can set the default location of that when installing it:

```sh
RUST_SRC_REPO=/path/to/rust cargo install cargo-bisect-rustc
```

See [Rust source repo] for more about configuring how `cargo-bisect-rustc` retrieves this information.

[Rust source repo]: rust-src-repo.md

# Rust source repo

For `cargo-bisect-rustc` to work, it needs to be able to read the git log of the [`rust-lang/rust`] repo.
`cargo-bisect-rustc` supports several methods for this described below.

## GitHub API

By default, `cargo-bisect-rustc` uses the GitHub API to fetch the information instead of using a local checkout.

```sh
cargo bisect-rustc --access=github
```

Beware that GitHub has restrictive rate limits for unauthenticated requests.
It allows 60 requests per hour, and `cargo-bisect-rustc` will use about 10 requests each time you run it (which can vary depending on the bisection).
If you run into the rate limit, you can raise it to 5000 requests per hour by setting the `GITHUB_TOKEN` environment variable to a [GitHub personal token].
If you use the [`gh` CLI tool], you can use it to get a token:

```sh
GITHUB_TOKEN=`gh auth token` cargo bisect-rustc --access=github
```

If you don't use `gh`, you'll just need to copy and paste the token.

## Local clone

`cargo-bisect-rustc` can also clone the rust repo in the current directory (as `rust.git`).
This option can be quite slow if you don't specify the repo path at build time.
You can specify this with the `--access` CLI argument:

## `RUST_SRC_REPO` environment variable

You can specify the location of the rust repo with the `RUST_SRC_REPO` environment variable at runtime.
This is useful if you already have it checked out somewhere, but is cumbersome to use.

```sh
RUST_SRC_REPO=/path/to/rust cargo bisect-rustc
```

## `RUST_SRC_REPO` environment variable (build-time)

Setting the `RUST_SRC_REPO` environment variable when installing `cargo-bisect-rustc` will set the default location for the rust repo.
This is recommended if you already have the rust repo checked out somewhere.

```sh
RUST_SRC_REPO=/path/to/rust cargo install cargo-bisect-rustc
```

[`rust-lang/rust`]: https://github.com/rust-lang/rust/
[GitHub personal token]: https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token
[`gh` CLI tool]: https://cli.github.com/

# Maintaining cargo-bisect-rustc

## Publishing

To publish a new release:

1. Create a PR to bump the version in `Cargo.toml` and `Cargo.lock`, and update [`CHANGELOG.md`](CHANGELOG.md).
2. After the merge is complete, create a new release. There are two approaches:
    - GUI: Create a new release in the UI, tag and title should be `v` and the version number. Copy a link to the changelog.
    - CLI: Run the following in the repo:
        ```bash
        VERSION="`cargo read-manifest | jq -r .version`" ; \
            gh release create -R rust-lang/cargo-bisect-rustc v$VERSION \
                --title v$VERSION \
                --notes "See https://github.com/rust-lang/cargo-bisect-rustc/blob/master/CHANGELOG.md#v${VERSION//.} for a complete list of changes."
        ```

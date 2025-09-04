# Alt builds

Each commit also generates what are called "alt" builds.
These are builds of rustc with some different options set.
As of August 2023, these include:

* `rust.parallel-compiler`
* `llvm.assertions`
* `rust.verify-llvm-ir`

For more information on these settings, see the [`config.toml` docs].
These alt settings are defined in [`ci/run.sh`].

Alt builds are only available for a few targets.
Look for the `-alt` builds in [`ci.yml`].

This can be useful if you are bisecting an LLVM issue.
With LLVM assertions enabled, alt builds have checks that can help identify broken assumptions.

Alt builds are only made for commit builds, and not nightly releases.
You will need to specify `--by-commit` (or use a hash in the `--start` or `--end` flags) to only use commit builds.

```sh
cargo bisect-rustc --alt --by-commit
```

[`config.toml` docs]: https://github.com/rust-lang/rust/blob/HEAD/config.example.toml
[`ci/run.sh`]: https://github.com/rust-lang/rust/blob/c0b6ffaaea3ebdf5f7a58fc4cf7ee52c91077fb9/src/ci/run.sh#L99-L105
[`ci.yml`]: https://github.com/rust-lang/rust/blob/HEAD/src/ci/github-actions/ci.yml

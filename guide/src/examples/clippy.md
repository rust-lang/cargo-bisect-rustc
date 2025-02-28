# Bisecting clippy

`cargo-bisect-rustc` can be used to check for Clippy regressions, too.
You'll need to instruct it to download clippy, and run the command correctly:

```sh
cargo bisect-rustc --start=1.67.0 --end=1.68.0 -c clippy -- clippy
```

Note that depending on what you are looking for, this may just find a PR that syncs the [`rust-clippy`] repo to `rust-lang/rust`.
You may be able to scan the list of changes in that PR to discover what you are looking for.
If the list of changes is too big or nothing is jumping out as a possible culprit, then consider using [`git bisect`] on the clippy repo itself (which will require building clippy).

To bisect a clippy warning, you can upgrade the warning to an error:

```sh
cargo bisect-rustc --start=1.84.0 --end=1.85.0 -c clippy -- clippy -- --forbid clippy::useless_conversion
```

[`rust-clippy`]: https://github.com/rust-lang/rust-clippy/
[`git bisect`]: https://git-scm.com/docs/git-bisect

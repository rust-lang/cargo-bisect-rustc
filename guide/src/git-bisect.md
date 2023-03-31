# Git bisect a custom build

There are some rare cases where you may need to build `rustc` with custom options, or otherwise work around issues with pre-built compilers not being available.
For this you can use [`git bisect`] to build the compiler locally.

It can be helpful to use the `--first-parent` option so that it only bisects the merge commits directly reachable on the master branch.
Otherwise the bisecting may land on intermediate commits from within a PR which may not build or test correctly.

To start the bisection, specifying the boundaries where the bisection will start:

```sh
git bisect start --first-parent
git bisect good 96ddd32c4bfb1d78f0cd03eb068b1710a8cebeef
git bisect bad a00f8ba7fcac1b27341679c51bf5a3271fa82df3
```

Then, build the compiler as needed and run your tests to check for a regression:

```sh
./x.py build std
rustc +stage1 foo.rs
```

You may want to consider running `./x.py clean` if you are running into issues since changes to the internal structures of build artifacts aren't always versioned, and those changes can be incompatible.
Incremental caches are particularly susceptible, so you may want to turn that off if you have turned them on.

If you determine the current version is good or bad, run `git bisect good` or `git bisect bad` to mark that, and then repeat building and marking until finished.

Similar to `cargo-bisect-rustc`, `git bisect` supports scripting and lots of other goodies.
Check out its documentation for more.

[`git bisect`]: https://git-scm.com/docs/git-bisect

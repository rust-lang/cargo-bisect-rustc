# Quick guidelines for tests

If you change the command line parameters of cargo-bisect, tests will fail, the crate `trycmd` is used to keep track of these changes.

In order to update files under `tests/cmd/*.{stdout,stderr}`, run the test generating the new expected results:

`TRYCMD=dump cargo test`

it will create a `dump` directory in the project root. Then move `dump/*.{stdout,stderr}` into `./tests/cmd` and run tests again. They should be all green now.

Note: if the local tests generate output specific for your machine, please replace that output with `[..]`, else CI tests will fail. Example:

``` diff
-      --host <HOST>             Host triple for the compiler [default: x86_64-unknown-linux-gnu]
+      --host <HOST>             Host triple for the compiler [default: [..]]
```

See the trycmd [documentation](https://docs.rs/trycmd/latest/trycmd/) for more info.

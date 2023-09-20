# Scripting on Windows

Using the `--script` option on Windows can be cumbersome because Windows does not support `#!` scripts like Unix does, and the built-in scripting can also be awkward.
The following sections show the different ways you can use scripting.

## Batch file

You can use DOS-style `.bat` files:

`test.bat`:
```bat
(cargo check 2>&1) | find "E0642"
```

This can be executed directly with:

```sh
cargo-bisect-rustc --script ./test.bat
```

But `.bat` can be challenging to do more complex options, or you may not be familiar with it.

## Powershell

You can't execute `.ps1` Powershell files directly, so you will need to use `pwsh` to launch them:

`test.ps1`:
```powershell
( cargo check 2>&1 ) | grep E0642
if ( -Not $? ) {
    exit 1
}
```

This can be run with:

```sh
cargo-bisect-rustc --script pwsh -- -File ./test.ps1
```

## Bash

If you have Git-for-Windows installed, then you can use its copy of bash to run bash scripts:

`test.sh`:
```sh
#!/bin/bash

cargo check 2>&1 | grep E0642
```

This can be run with:

```sh
cargo-bisect-rustc --script "C:\\Program Files\\Git\\usr\\bin\\bash.exe" -- ./test.sh
```

This also works if you have bash from something like msys2 installed.

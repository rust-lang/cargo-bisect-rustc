# Cargo Bisection

This tool bisects either Rust nightlies or CI artifacts.

## Setup

Optionally, set the `RUST_SRC_REPO` environment variable to point at your local copy of 
https://github.com/rust-lang/rust. If the variable is not defined, `cargo-bisect-rustc` will clone 
it for you which takes a while.

[**Tutorial**](TUTORIAL.md)

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

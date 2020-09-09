use std::env;

fn main() {
    println!("cargo:rustc-env=HOST={}", env::var("TARGET").unwrap());
}

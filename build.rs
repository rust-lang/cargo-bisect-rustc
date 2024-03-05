use std::env;

fn main() {
    println!("cargo:rustc-env=HOST={}", env::var("TARGET").unwrap());
    // Prevents cargo from scanning the whole directory for changes.
    println!("cargo:rerun-if-changed=build.rs");
}

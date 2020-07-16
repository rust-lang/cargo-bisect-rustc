// Strategy inspired by dtolnay/rustversion: run `rustc --version` at build time
// to observe version info.
//
// (The dtolnay/rustversion is dual-licensed under APACHE/MIT as of January 2020.)

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{self, Command};

#[derive(PartialOrd, Ord, PartialEq, Eq, Debug)]
struct YearMonthDay(u32, u32, u32);

enum DeltaKind { Fix, Err }

fn main() {
    let mut context = Context::introspect();
    context.generate();
}

struct Context {
    commit: Commit,
    rustc_date: YearMonthDay,
}

#[derive(PartialOrd, Ord, PartialEq, Eq, Debug)]
struct Commit(String);

impl Context {
    fn introspect() -> Context {
        let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
        let output = Command::new(&rustc).arg("--version").output().unwrap_or_else(|e| {
            let rustc = rustc.to_string_lossy();
            eprintln!("Error: failed to run `{} --version`: {}", rustc, e);
            process::exit(1);
        });
        let output = String::from_utf8(output.stdout).unwrap();
        let mut tokens = output.split(' ');

        let _rustc = tokens.next().unwrap();
        let _version = tokens.next().unwrap();
        let open_paren_commit = tokens.next().unwrap();
        let date_close_paren = tokens.next().unwrap();

        let commit = Commit(open_paren_commit[1..].to_string());

        let date_str: String =
            date_close_paren.matches(|c: char| c.is_numeric() || c == '-').collect();
        let mut date_parts = date_str.split('-');
        let year: u32 = date_parts.next().unwrap().parse().unwrap();
        let month: u32 = date_parts.next().unwrap().parse().unwrap();
        let day: u32 = date_parts.next().unwrap().parse().unwrap();

        Context { commit, rustc_date: YearMonthDay(year, month, day) }
    }

    fn generate(&mut self) {
        let inject_with_error = match DELTA_KIND {
            DeltaKind::Err => self.rustc_date >= DELTA_DATE,
            DeltaKind::Fix => self.rustc_date < DELTA_DATE,
        };
        let prefix = if inject_with_error { "#[rustc_error] " } else { "" };
        let maybe_static_error = format!("{PREFIX}{ITEM}", PREFIX=prefix, ITEM="fn main() { }");

        let content = format!(r#"{MAIN}
pub const COMMIT: &'static str = "{COMMIT}";
pub const DATE: &'static str = "{Y:04}-{M:02}-{D:02}";
"#,
                              MAIN=maybe_static_error,
                              COMMIT=self.commit.0,
                              Y=self.rustc_date.0,
                              M=self.rustc_date.1,
                              D=self.rustc_date.2);

        let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR not set");
        let out_file = Path::new(&out_dir).join("version.rs");
        fs::write(out_file, content).expect("failed to write version.rs");
    }
}

use std::fs::{DirBuilder};
use std::path::{Path};

pub struct InjectionPoint {
    pub date: YearMonthDay,
    pub associated_sha: &'static str,
}

pub struct Test<'a> {
    pub crate_name: &'a str,
    pub cli_params: &'a [&'a str],
    pub delta_date: InjectionPoint,
    pub delta_kind: DeltaKind,
}

impl<'a> Test<'a> {
    pub fn expected_sha(&self) -> &str {
        self.delta_date.associated_sha
    }
}

pub fn make_crate_files(
    dir_builder: &DirBuilder,
    dir: &Path,
    test: &Test)
    -> Result<(), failure::Error>
{
    (crate::make_a_crate::Crate {
        dir,
        name: test.crate_name,
        build_rs: Some(meta_build(test).into()),
        cargo_toml: format!(r##"
[package]
name = "{NAME}"
version = "0.1.0"
authors = ["Felix S. Klock II <pnkfelix@pnkfx.org>"]
"##, NAME=test.crate_name).into(),
        main_rs: MAIN_RS.into(),
    }).make_files(dir_builder)?;

    Ok(())
}

// A test crate to exercise `cargo-bisect-rustc` has three basic components: a
// Cargo.toml file, a build.rs script that inspects the current version of Rust
// and injects an error for the appropriate versions into a build-time generated
// version.rs file, and a main.rs file that include!'s the version.rs file
//
// We only inject errors based on YYYY-MM-DD date comparison (<, <=, >=, >), and
// having that conditonally add a `#[rustc_error]` to the (injected) `fn main()`
// function.

const MAIN_RS: &'static str = std::include_str!("meta_build/included_main.rs");

#[derive(Copy, Clone)]
pub struct YearMonthDay(pub u32, pub u32, pub u32);

#[derive(Copy, Clone)]
pub enum DeltaKind { Fix, Err }

fn meta_build(test: &Test) -> String {
    let YearMonthDay(year, month, day) = test.delta_date.date;
    let delta_kind = test.delta_kind;
    let date_item = format!(r##"
/// `DELTA_DATE` identfies nightly where simulated change was injected.
const DELTA_DATE: YearMonthDay = YearMonthDay({YEAR}, {MONTH}, {DAY});
"##,
                            YEAR=year, MONTH=month, DAY=day);

    let kind_variant = match delta_kind {
        DeltaKind::Fix => "Fix",
        DeltaKind::Err => "Err",
    };
    let kind_item = format!(r##"
/// `DELTA_KIND` identfies whether simulated change is new error, or a fix to ancient error.
const DELTA_KIND: DeltaKind = DeltaKind::{VARIANT};
"##,
                            VARIANT=kind_variant);

    format!("{DATE_ITEM}{KIND_ITEM}{SUFFIX}",
            DATE_ITEM=date_item, KIND_ITEM=kind_item, SUFFIX=BUILD_SUFFIX)
}

const BUILD_SUFFIX: &'static str = std::include_str!("meta_build/included_build_suffix.rs");

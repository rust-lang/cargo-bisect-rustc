const CRATE_NAME: &'static str = "eventually_ice";
const INJECTION_COMMIT: &'static str = "6af388b25050bca26710be7e4030e17bf6d8d2f7";
const INJECTION_LOWER_BOUND: &'static str = "2020-02-20";
const INJECTION_UPPER_BOUND: &'static str = "2020-02-22";

// This main.rs captures a bit of code that has a known ICE that was
// introduced relatively recently, as reported in issue #69615.
const ICE_MAIN_RS: &'static str = std::include_str!("icy_code/included_main.rs");

// This test crate encodes a case of an internal compiler error (ICE) that was
// injected relatively recently (see `ICE_MAIN_RS` below). The intention is to
// use this to test the handling of `--regress=ice` option to `rustc`.
//
// Note that the main intention of the test is to capture the distinction
// between flagging a static error in the program from signalling an internal
// error. That is: Using an example here of a *correct* program that causes an
// ICE is not a great test, because it wouldn't necessarily be testing the
// important effect of the `--regress=ice` flag.
//
// In the long term, since we only store binaries over a fixed length of time,
// this test will need to be updated with new examples of ICE's. (For now it
// seems safe to assume that the compiler will always have *some* example
// program that can be used to observe an ICE.)

const CARGO_TOML: &'static str = r##"
[package]
name = "eventually-ice"
version = "0.1.0"
authors = ["Felix S. Klock II <pnkfelix@pnkfx.org>"]
"##;

mod common {
    pub(crate) mod command_invocation;
    pub(crate) mod make_a_crate;
    pub(crate) mod which_temp;
}

pub(crate) use self::common::command_invocation;
pub(crate) use self::common::make_a_crate;
pub(crate) use self::common::which_temp;

use self::which_temp::{WhichTempDir, WhichTempDirectory};

#[test]
fn ice_test() -> Result<(), failure::Error> {
    test_ice_core::<WhichTempDir>()
}

fn test_ice_core<WhichTemp>() -> Result<(), failure::Error>
where
    WhichTemp: WhichTempDirectory,
{
    let root = WhichTemp::root()?;
    let tmp_dir = WhichTemp::target(&root);
    let dir = tmp_dir.join(CRATE_NAME);

    let dir_builder = WhichTemp::dir_builder();

    (make_a_crate::Crate {
        dir: &dir,
        name: CRATE_NAME,
        build_rs: None,
        cargo_toml: CARGO_TOML.into(),
        main_rs: ICE_MAIN_RS.into(),
    })
    .make_files(&dir_builder)?;

    let mut cmd = command_invocation::Context {
        cli_params: &[
            "--preserve",
            "--regress=ice",
            "--access=github",
            "--start",
            INJECTION_LOWER_BOUND,
            "--end",
            INJECTION_UPPER_BOUND,
        ],
        dir: dir.as_path(),
    };

    let command_invocation::Output {
        status: _,
        stderr,
        stdout,
    } = cmd.run()?;

    println!(
        "Command output stdout for {}: \n```\n{}\n```",
        CRATE_NAME, stdout
    );
    println!(
        "Command output stderr for {}: \n```\n{}\n```",
        CRATE_NAME, stderr
    );

    // The most basic check: does the output actually tell us about the
    // "regressing" commit.
    let needle = format!("Regression in {}", INJECTION_COMMIT);
    // println!("searching for {:?} in stdout: {:?} stderr: {:?}", needle, stdout, stderr);
    assert!(stderr.contains(&needle));

    Ok(())
}

const INJECTION_COMMIT: &'static str = "f8fd4624474a68bd26694eff3536b9f3a127b2d3";
const INJECTION_LOWER_BOUND: &'static str = "2020-02-06";
const INJECTION_UPPER_BOUND: &'static str = "2020-02-08";

const INJECTION_POINT: InjectionPoint = InjectionPoint {
    date: YearMonthDay(2020, 02, 07),
    associated_sha: INJECTION_COMMIT,
};

mod cli {
    pub(crate) mod meta_build;
}

pub(crate) use self::cli::meta_build;

mod common {
    pub(crate) mod command_invocation;
    pub(crate) mod make_a_crate;
    pub(crate) mod which_temp;
}

pub(crate) use self::common::command_invocation;
pub(crate) use self::common::make_a_crate;
pub(crate) use self::common::which_temp;

use self::meta_build::{DeltaKind, InjectionPoint, Test, YearMonthDay};
use self::which_temp::{WhichTempDir, WhichTempDirectory};

// These tests pass `--preserve` and `--access=github` because that is the best
// way to try to ensure that the tests complete as quickly as possible.

pub const BASIC_TEST: Test = Test {
    crate_name: "cbr_test_cli_basic",
    cli_params: &["--preserve", "--access=github",
                  "--start", INJECTION_LOWER_BOUND, "--end", INJECTION_UPPER_BOUND],
    delta_date: INJECTION_POINT,
    delta_kind: DeltaKind::Err,
};

pub const FIXED_TEST: Test = Test {
    crate_name: "cbr_test_cli_fixed",
    cli_params: &["--regress=success",
                  "--preserve", "--access=github",
                  "--start", INJECTION_LOWER_BOUND, "--end", INJECTION_UPPER_BOUND],
    delta_date: INJECTION_POINT,
    delta_kind: DeltaKind::Fix,
};

// Ordinarily, I would put both of these tests into separate `#[test]` methods.
// However, if you do that, then `cargo test` will run them in parallel, and you
// end up with `cargo-bisect-rustc` racing to install the toolchains it
// downloads.
//
// (It is arguably a bug that we do not gracefully handle this situation.)
//
// In any case, the simplest fix for the test infrastructure is to ensure that
// no tests overlap in the range of dates they search for a regression.
#[test]
fn cli_test() -> Result<(), failure::Error> {
    test_cli_core::<WhichTempDir>(&BASIC_TEST)?;
    test_cli_core::<WhichTempDir>(&FIXED_TEST)?;
    Ok(())
}

fn test_cli_core<WhichTemp>(test: &meta_build::Test) -> Result<(), failure::Error>
where WhichTemp: WhichTempDirectory
{
    let root = WhichTemp::root()?;
    let tmp_dir = WhichTemp::target(&root);
    let dir = tmp_dir.join(test.crate_name);

    let dir_builder = WhichTemp::dir_builder();
    meta_build::make_crate_files(&dir_builder, &dir, test)?;

    let mut cmd = command_invocation::Context {
        cli_params: test.cli_params,
        dir: dir.as_path(),
    };

    let command_invocation::Output { status: _, stderr, stdout } = cmd.run()?;

    println!("Command output stdout for {}: \n```\n{}\n```", test.crate_name, stdout);
    println!("Command output stderr for {}: \n```\n{}\n```", test.crate_name, stderr);

    // The most basic check: does the output actually tell us about the
    // "regressing" commit.
    let needle = format!("Regression in {}", test.expected_sha());
    // println!("searching for {:?} in stdout: {:?} stderr: {:?}", needle, stdout, stderr);
    assert!(stderr.contains(&needle));

    Ok(())
}

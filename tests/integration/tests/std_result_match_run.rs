//! V0-064 `std_result_match` fixture build and VM run.

#![allow(clippy::expect_used)]

use phx_bytecode::verify;
use phx_test::{ExpectedLocal, assert_main_locals, ensure_built_project, require_cli_project};
use phx_vm::run;

#[test]
fn std_result_match_fixture_runs() {
    require_cli_project("std_result_match");
    let built = ensure_built_project("std_result_match");
    let verified = verify(&built.module).expect("verify");

    run(verified).expect("run std_result_match");
}

#[test]
fn std_result_match_reads_forty_two() {
    require_cli_project("std_result_match");
    let built = ensure_built_project("std_result_match");
    assert_main_locals(&built.module, &[(2, ExpectedLocal::S32(42))]);
}

//! V0-064 `std_result_match` fixture build and VM run.

#![allow(clippy::expect_used)]

use phx_bytecode::verify;
use phx_compiler::{BuildOptions, build_project, load_project_binary};
use phx_test::{
    ExpectedLocal, assert_main_locals, discover_cli_project, fixture_fs_lock, require_cli_project,
};
use phx_vm::run;

#[test]
fn std_result_match_fixture_runs() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_result_match");
    let config = discover_cli_project(&root);
    build_project(&config, None, BuildOptions::force(true)).expect("build std_result_match");
    let module = load_project_binary(&config).expect("load bytecode");
    let verified = verify(&module).expect("verify");

    run(verified).expect("run std_result_match");
}

#[test]
fn std_result_match_reads_forty_two() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_result_match");
    let config = discover_cli_project(&root);
    build_project(&config, None, BuildOptions::force(true)).expect("build std_result_match");
    let module = load_project_binary(&config).expect("load bytecode");
    assert_main_locals(&module, &[(2, ExpectedLocal::S32(42))]);
}

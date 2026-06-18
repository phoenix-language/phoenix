//! V0-067 Phase 7 capstone — Result match, trait defaults, and heap slices in one program.

#![allow(clippy::expect_used)]

use phx_bytecode::verify;
use phx_compiler::{BuildOptions, build_project, load_project_binary};
use phx_test::{ExpectedLocal, assert_main_locals, discover_cli_project, require_cli_project};
use phx_vm::run;

#[test]
fn std_platform_smoke_fixture_runs() {
    let root = require_cli_project("std_platform_smoke");
    let config = discover_cli_project(&root);
    build_project(&config, None, BuildOptions::force(true)).expect("build std_platform_smoke");
    let module = load_project_binary(&config).expect("load bytecode");
    let verified = verify(&module).expect("verify");

    run(verified).expect("run std_platform_smoke");
}

#[test]
fn std_platform_smoke_combines_result_trait_and_heap_slice() {
    let root = require_cli_project("std_platform_smoke");
    let config = discover_cli_project(&root);
    build_project(&config, None, BuildOptions::force(true)).expect("build std_platform_smoke");
    let module = load_project_binary(&config).expect("load bytecode");
    // `sum` = Config.value (42) + inherited marker_byte (77)
    assert_main_locals(&module, &[(2, ExpectedLocal::S32(119))]);
}

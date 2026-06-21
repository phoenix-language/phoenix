//! V0-067 Phase 7 capstone — Result match, trait defaults, and heap slices in one program.

#![allow(clippy::expect_used)]

use phx_bytecode::verify;
use phx_test::{ExpectedLocal, assert_main_locals, ensure_built_project, require_cli_project};
use phx_vm::run;

#[test]
fn std_platform_smoke_fixture_runs() {
    require_cli_project("std_platform_smoke");
    let built = ensure_built_project("std_platform_smoke");
    let verified = verify(&built.module).expect("verify std_platform_smoke");

    run(verified).expect("run std_platform_smoke");
}

#[test]
fn std_platform_smoke_combines_result_trait_and_heap_slice() {
    require_cli_project("std_platform_smoke");
    let built = ensure_built_project("std_platform_smoke");
    // `sum` = Config.value (42) + inherited marker_byte (77)
    assert_main_locals(&built.module, &[(2, ExpectedLocal::S32(119))]);
}

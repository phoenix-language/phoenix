//! `DynamicArray` std smoke fixtures.

#![allow(clippy::expect_used)]

use phx_test::{fixture_fs_lock, force_build_project, require_cli_project};
use phx_vm::run;

#[test]
fn dynamic_array_smoke_fixture_runs() {
    let _lock = fixture_fs_lock();
    require_cli_project("dynamic_array_smoke");
    let built = force_build_project("dynamic_array_smoke");
    let verified = phx_bytecode::verify(&built.module).expect("verify dynamic_array_smoke");

    run(verified).expect("run dynamic_array_smoke");
}

#[test]
fn dynamic_array_drop_smoke_fixture_runs() {
    let _lock = fixture_fs_lock();
    require_cli_project("dynamic_array_drop_smoke");
    let built = force_build_project("dynamic_array_drop_smoke");
    let verified = phx_bytecode::verify(&built.module).expect("verify dynamic_array_drop_smoke");

    run(verified).expect("run dynamic_array_drop_smoke");
}

/// Verifier enforces canonical operand-stack depth at `Return` (PHX-035); drop glue on std
/// `DynamicArray` must not leak stack slots.
#[test]
fn dynamic_array_drop_smoke_verifies_balanced_main_return() {
    let _lock = fixture_fs_lock();
    require_cli_project("dynamic_array_drop_smoke");
    let built = force_build_project("dynamic_array_drop_smoke");
    phx_bytecode::verify(&built.module).expect("verify balanced main return stack for drop glue");
}

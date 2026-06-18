//! `DynamicArray` std semantics fixtures (PHX-061).

#![allow(clippy::expect_used)]

use phx_compiler::{CompileError, check_file};
use phx_diagnostics::TypeCheckError;
use phx_test::{
    ExpectedLocal, assert_main_locals, cli_project_main, ensure_built_project, require_cli_project,
};
use phx_vm::{VmErrorKind, run};

#[test]
fn dynamic_array_smoke_fixture_runs() {
    require_cli_project("dynamic_array_smoke");
    let built = ensure_built_project("dynamic_array_smoke");
    let verified = phx_bytecode::verify(&built.module).expect("verify dynamic_array_smoke");

    run(verified).expect("run dynamic_array_smoke");
}

#[test]
fn dynamic_array_smoke_push_get_scalar_locals() {
    require_cli_project("dynamic_array_smoke");
    let built = ensure_built_project("dynamic_array_smoke");
    // `check_sum` / `check_len` locals after push+get smoke.
    assert_main_locals(
        &built.module,
        &[(4, ExpectedLocal::S32(33)), (6, ExpectedLocal::U32(3))],
    );
}

#[test]
fn dynamic_array_drop_smoke_fixture_runs() {
    require_cli_project("dynamic_array_drop_smoke");
    let built = ensure_built_project("dynamic_array_drop_smoke");
    let verified = phx_bytecode::verify(&built.module).expect("verify dynamic_array_drop_smoke");

    run(verified).expect("run dynamic_array_drop_smoke");
}

/// Verifier enforces canonical operand-stack depth at `Return` (PHX-035); drop glue on std
/// `DynamicArray` must not leak stack slots.
#[test]
fn dynamic_array_drop_smoke_verifies_balanced_main_return() {
    require_cli_project("dynamic_array_drop_smoke");
    let built = ensure_built_project("dynamic_array_drop_smoke");
    phx_bytecode::verify(&built.module).expect("verify balanced main return stack for drop glue");
}

#[test]
fn dynamic_array_grow_fixture_runs() {
    require_cli_project("dynamic_array_grow");
    let built = ensure_built_project("dynamic_array_grow");
    let verified = phx_bytecode::verify(&built.module).expect("verify dynamic_array_grow");

    run(verified).expect("run dynamic_array_grow");
}

#[test]
fn dynamic_array_grow_doubles_capacity_and_sums_elements() {
    require_cli_project("dynamic_array_grow");
    let built = ensure_built_project("dynamic_array_grow");
    // `check_sum` / `check_len` after eight pushes (forces grow past cap=4).
    assert_main_locals(
        &built.module,
        &[(10, ExpectedLocal::S32(36)), (11, ExpectedLocal::U32(8))],
    );
}

#[test]
fn dynamic_array_pop_fixture_runs() {
    require_cli_project("dynamic_array_pop");
    let built = ensure_built_project("dynamic_array_pop");
    let verified = phx_bytecode::verify(&built.module).expect("verify dynamic_array_pop");

    run(verified).expect("run dynamic_array_pop");
}

#[test]
fn dynamic_array_pop_returns_popped_values_and_remaining_len() {
    require_cli_project("dynamic_array_pop");
    let built = ensure_built_project("dynamic_array_pop");
    assert_main_locals(
        &built.module,
        &[
            (1, ExpectedLocal::S32(30)),
            (2, ExpectedLocal::S32(20)),
            (3, ExpectedLocal::U32(1)),
            (4, ExpectedLocal::S32(10)),
        ],
    );
}

#[test]
fn dynamic_array_index_oob_fails_at_runtime() {
    require_cli_project("dynamic_array_index_oob");
    let built = ensure_built_project("dynamic_array_index_oob");
    let verified = phx_bytecode::verify(&built.module).expect("verify dynamic_array_index_oob");
    let err = run(verified).expect_err("OOB dynamic_array index");
    assert!(
        matches!(err.kind, VmErrorKind::FieldOutOfRange),
        "expected FieldOutOfRange, got {err:?}"
    );
}

#[test]
fn dynamic_array_nested_drop_fixture_runs() {
    require_cli_project("dynamic_array_nested_drop");
    let built = ensure_built_project("dynamic_array_nested_drop");
    let verified = phx_bytecode::verify(&built.module).expect("verify dynamic_array_nested_drop");

    run(verified).expect("run dynamic_array_nested_drop");
}

#[test]
fn dynamic_array_move_in_use_after_move_errors() {
    let path = cli_project_main("dynamic_array_move_in");
    let Err(CompileError::TypeCheck { bag, .. }) = check_file(&path) else {
        panic!("expected use-after-move when reusing value moved into DynamicArray");
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::UseAfterMove { .. })),
        "expected UseAfterMove: {:?}",
        bag.errors()
    );
}

#[test]
fn dynamic_array_uaf_fails_at_runtime() {
    require_cli_project("dynamic_array_uaf");
    let built = ensure_built_project("dynamic_array_uaf");
    let verified = phx_bytecode::verify(&built.module).expect("verify dynamic_array_uaf");
    let err = run(verified).expect_err("use after free should fail");
    assert!(
        matches!(err.kind, VmErrorKind::UseAfterFree),
        "expected UseAfterFree, got {err:?}"
    );
}

#[test]
fn dynamic_array_double_free_fails_at_runtime() {
    require_cli_project("dynamic_array_double_free");
    let built = ensure_built_project("dynamic_array_double_free");
    let verified = phx_bytecode::verify(&built.module).expect("verify dynamic_array_double_free");
    let err = run(verified).expect_err("double free should fail");
    assert!(
        matches!(
            err.kind,
            VmErrorKind::DoubleFree | VmErrorKind::UseAfterFree
        ),
        "expected DoubleFree or UseAfterFree, got {err:?}"
    );
}

//! `UniquePtr` std semantics fixtures.

#![allow(clippy::expect_used)]

use phx_compiler::{CompileError, check_file};
use phx_diagnostics::TypeCheckError;
use phx_test::{
    ExpectedLocal, assert_main_locals, cli_project_main, ensure_built_project, require_cli_project,
};
use phx_vm::{VmErrorKind, run};

#[test]
fn unique_ptr_smoke_fixture_runs() {
    require_cli_project("unique_ptr_smoke");
    let built = ensure_built_project("unique_ptr_smoke");
    let verified = phx_bytecode::verify(&built.module).expect("verify unique_ptr_smoke");

    run(verified).expect("run unique_ptr_smoke");
}

#[test]
fn unique_ptr_smoke_get_scalar_local() {
    require_cli_project("unique_ptr_smoke");
    let built = ensure_built_project("unique_ptr_smoke");
    assert_main_locals(&built.module, &[(2, ExpectedLocal::S32(42))]);
}

#[test]
fn unique_ptr_drop_smoke_fixture_runs() {
    require_cli_project("unique_ptr_drop_smoke");
    let built = ensure_built_project("unique_ptr_drop_smoke");
    let verified = phx_bytecode::verify(&built.module).expect("verify unique_ptr_drop_smoke");

    run(verified).expect("run unique_ptr_drop_smoke");
}

/// Verifier enforces canonical operand-stack depth at `Return` (PHX-035); drop glue on std
/// `UniquePtr` must not leak stack slots.
#[test]
fn unique_ptr_drop_smoke_verifies_balanced_main_return() {
    require_cli_project("unique_ptr_drop_smoke");
    let built = ensure_built_project("unique_ptr_drop_smoke");
    phx_bytecode::verify(&built.module).expect("verify balanced main return stack for drop glue");
}

#[test]
fn unique_ptr_move_fixture_runs() {
    require_cli_project("unique_ptr_move");
    let built = ensure_built_project("unique_ptr_move");
    let verified = phx_bytecode::verify(&built.module).expect("verify unique_ptr_move");

    run(verified).expect("run unique_ptr_move");
}

#[test]
fn unique_ptr_move_in_use_after_move_errors() {
    let path = cli_project_main("unique_ptr_move_in");
    let Err(CompileError::TypeCheck { bag, .. }) = check_file(&path) else {
        panic!("expected use-after-move when reusing UniquePtr after move");
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
fn unique_ptr_uaf_fails_at_runtime() {
    require_cli_project("unique_ptr_uaf");
    let built = ensure_built_project("unique_ptr_uaf");
    let verified = phx_bytecode::verify(&built.module).expect("verify unique_ptr_uaf");
    let err = run(verified).expect_err("use after free should fail");
    assert!(
        matches!(err.kind, VmErrorKind::UseAfterFree),
        "expected UseAfterFree, got {err:?}"
    );
}

#[test]
fn unique_ptr_double_free_fails_at_runtime() {
    require_cli_project("unique_ptr_double_free");
    let built = ensure_built_project("unique_ptr_double_free");
    let verified = phx_bytecode::verify(&built.module).expect("verify unique_ptr_double_free");
    let err = run(verified).expect_err("double free should fail");
    assert!(
        matches!(
            err.kind,
            VmErrorKind::DoubleFree | VmErrorKind::UseAfterFree
        ),
        "expected DoubleFree or UseAfterFree, got {err:?}"
    );
}

#[test]
fn unique_ptr_nested_drop_fixture_runs() {
    require_cli_project("unique_ptr_nested_drop");
    let built = ensure_built_project("unique_ptr_nested_drop");
    let verified = phx_bytecode::verify(&built.module).expect("verify unique_ptr_nested_drop");

    run(verified).expect("run unique_ptr_nested_drop");
}

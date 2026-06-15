//! V0-054: scope-end drop glue smoke.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_compiler::{CompileError, check_file};
use phx_diagnostics::TypeCheckError;
use phx_test::{assert_fixture_exists, cli_fixture, compile_fixture, run_fixture_smoke};

#[test]
fn drop_fixture_compiles_and_verifies() {
    let _ = compile_fixture("drop.phx");
}

#[test]
fn drop_fixture_runs_without_runtime_error() {
    run_fixture_smoke("drop.phx");
}

#[test]
fn drop_double_user_drop_errors() {
    // Manual `drop(self)` consumes the receiver like any by-value method (`ownership.md`).
    let path = cli_fixture("drop_double.phx");
    assert_fixture_exists(&path);
    let Err(CompileError::TypeCheck { bag, .. }) = check_file(&path) else {
        panic!("expected use-after-move on second `w.drop()`");
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
fn drop_use_after_user_drop_errors() {
    let path = cli_fixture("drop_use_after.phx");
    assert_fixture_exists(&path);
    let Err(CompileError::TypeCheck { bag, .. }) = check_file(&path) else {
        panic!("expected use-after-move after manual `w.drop()`");
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::UseAfterMove { .. })),
        "expected UseAfterMove: {:?}",
        bag.errors()
    );
}

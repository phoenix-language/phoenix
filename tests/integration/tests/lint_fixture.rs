//! Lint tests using on-disk fixtures (migrated from phx-compiler/tests/lint.rs).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_compiler::{CompileError, check_file};
use phx_diagnostics::TypeCheckError;
use phx_test::cli_project_main;

#[test]
fn discarded_std_result_is_type_error() {
    let path = cli_project_main("lint_std_result_discard");
    let err = check_file(&path).expect_err("expected type-check failure");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::DiscardedStdResult { .. })),
        "expected DiscardedStdResult, got {bag}"
    );
}

#[test]
fn discarded_std_option_is_type_error() {
    let path = cli_project_main("lint_std_option_discard");
    let err = check_file(&path).expect_err("expected type-check failure");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::DiscardedStdOption { .. })),
        "expected DiscardedStdOption, got {bag}"
    );
}

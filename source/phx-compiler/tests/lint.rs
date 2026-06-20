//! Lint pass tests (typed must-use and expr span types).

mod support;

use phx_compiler::{CompileError, check_file, compile_source, lint_checked};
use phx_diagnostics::{LintKind, TypeCheckError};
use support::{project_main_path, test_err, test_ok};

fn lint_source(source: &str) -> phx_diagnostics::LintBag {
    let unit = test_ok(compile_source(source, None), "compile");
    test_ok(lint_checked(&unit.typed), "lint")
}

#[test]
fn expr_span_types_populated() {
    let unit = test_ok(
        compile_source("main :: () => { const x: s32 = 1; };", None),
        "compile",
    );
    assert!(
        !unit.typed.expr_span_types.is_empty(),
        "expected expr_span_types entries after type-check"
    );
}

#[test]
fn user_enum_discard_no_std_must_use() {
    let lints = lint_source(
        "Pair :: <a, b> enum { Ok(a), Err(b), }; get :: () => Pair<s32, s32> { Ok(1) }; main :: () => { get(); const _ = 0; };",
    );
    assert!(
        !lints
            .lints()
            .iter()
            .any(|loc| loc.lint.kind == LintKind::MustUse),
        "user enum discard should not trigger std Result/Option lint: {lints}"
    );
}

#[test]
fn std_result_discard_is_not_lint() {
    let path = project_main_path("lint_std_result_discard");
    let err = test_err(check_file(&path), "expected type-check failure");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::DiscardedStdResult { .. })),
        "discarded std Result should be a type error, not a lint: {bag}"
    );
}

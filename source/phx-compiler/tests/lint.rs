//! Lint pass tests (typed must-use and expr span types).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use phx_compiler::{CompileError, check_file, compile_source, lint_checked};
use phx_diagnostics::{LintKind, TypeCheckError};
use support::project_main_path;

fn lint_source(source: &str) -> phx_diagnostics::LintBag {
    let unit = compile_source(source, None).expect("compile");
    lint_checked(&unit.typed).expect("lint")
}

#[test]
fn expr_span_types_populated() {
    let unit = compile_source("main :: () => { const x: s32 = 1; };", None).expect("compile");
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
    let err = check_file(&path).expect_err("expected type-check failure");
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

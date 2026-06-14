//! Lint pass tests (typed must-use and expr span types).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_compiler::{check_file, compile_source, lint_checked};
use phx_diagnostics::LintKind;

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
fn discarded_std_result_emits_must_use() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/lint_std_result_discard/src/main.phx");
    if !path.is_file() {
        return;
    }
    let unit = check_file(&path).expect("check");
    let lints = lint_checked(&unit.typed).expect("lint");
    assert!(
        lints.lints().iter().any(|loc| {
            loc.lint.kind == LintKind::MustUse && loc.lint.message.contains("Result")
        }),
        "expected discarded std Result must-use warning, got {lints}"
    );
}

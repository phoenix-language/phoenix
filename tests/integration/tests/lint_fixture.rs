//! Lint tests using on-disk fixtures (migrated from phx-compiler/tests/lint.rs).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_compiler::{check_file, lint_checked};
use phx_diagnostics::LintKind;
use phx_test::cli_project_main;

#[test]
fn discarded_std_result_emits_must_use() {
    let path = cli_project_main("lint_std_result_discard");
    let unit = check_file(&path).expect("check");
    let lints = lint_checked(&unit.typed).expect("lint");
    assert!(
        lints.lints().iter().any(|loc| {
            loc.lint.kind == LintKind::MustUse && loc.lint.message.contains("Result")
        }),
        "expected discarded std Result must-use warning, got {lints}"
    );
}

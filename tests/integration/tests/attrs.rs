//! Integration tests for `#[...]` item attributes (V0-039).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_compiler::{check_file, lint_checked};
use phx_diagnostics::LintKind;
use phx_test::{ExpectedLocal, assert_main_locals, check_fixture_ok, cli_fixture, compile_fixture};

#[test]
fn cfg_strips_inactive_top_level_item() {
    check_fixture_ok("attr_cfg_strip.phx");
    let module = compile_fixture("attr_cfg_strip.phx");
    assert_main_locals(&module, &[(0, ExpectedLocal::S32(7))]);
}

#[test]
fn deprecated_allow_suppresses_warning() {
    let path = cli_fixture("attr_deprecated_allow.phx");
    let unit = check_file(&path).expect("check");
    let lints = lint_checked(&unit.typed).expect("lint");
    assert!(
        !lints.has_lints(),
        "expected no warnings with #[allow(deprecated)], got {lints}"
    );
}

#[test]
fn deprecated_use_emits_warning() {
    let path = cli_fixture("attr_deprecated_warn.phx");
    let unit = check_file(&path).expect("check");
    let lints = lint_checked(&unit.typed).expect("lint");
    assert!(
        lints
            .lints()
            .iter()
            .any(|loc| loc.lint.kind == LintKind::Deprecated),
        "expected deprecated warning, got {lints}"
    );
}

#[test]
fn must_use_discard_emits_warning() {
    let path = cli_fixture("attr_must_use.phx");
    let unit = check_file(&path).expect("check");
    let lints = lint_checked(&unit.typed).expect("lint");
    assert!(
        lints
            .lints()
            .iter()
            .any(|loc| loc.lint.kind == LintKind::MustUse),
        "expected must_use warning, got {lints}"
    );
}

#[test]
fn keyword_directives_still_compile() {
    check_fixture_ok("sample.phx");
}

#[test]
fn bracket_derive_attribute_parses() {
    let path = cli_fixture("attr_bracket_derive.phx");
    let source = std::fs::read_to_string(&path).expect("read fixture");
    let bag = phx_test::expect_typeck_err(&source);
    let msg = bag.to_string();
    assert!(
        msg.contains("#derive") || msg.contains("UnsupportedFeature"),
        "expected derive rejection after parse, got: {msg}"
    );
}

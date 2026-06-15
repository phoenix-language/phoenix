//! Golden diagnostic output tests (formatted compiler messages).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use phx_test::{
    assert_golden, cli_fixture, cli_modules_dir, format_check_file, format_check_with_module_root,
    format_compile_source, integration_diagnostics_dir,
};

fn diagnostics_dir() -> PathBuf {
    integration_diagnostics_dir(&PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

#[test]
fn golden_bad_type_mismatch() {
    let path = cli_fixture("bad_type.phx");
    assert_golden(&diagnostics_dir(), "bad_type", &format_check_file(&path));
}

#[test]
fn golden_use_after_move_note() {
    let path = cli_fixture("use_after_move.phx");
    assert_golden(
        &diagnostics_dir(),
        "use_after_move",
        &format_check_file(&path),
    );
}

#[test]
fn golden_missing_main() {
    let path = cli_fixture("missing_main.phx");
    assert_golden(
        &diagnostics_dir(),
        "missing_main",
        &format_check_file(&path),
    );
}

#[test]
fn golden_import_cycle() {
    let module_root = cli_modules_dir();
    let entry = module_root.join("cycle_a.phx");
    assert_golden(
        &diagnostics_dir(),
        "import_cycle",
        &format_check_with_module_root(&entry, &module_root),
    );
}

#[test]
fn golden_multi_resolve_duplicate() {
    let path = diagnostics_dir().join("multi_resolve_duplicate.phx");
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_golden(
        &diagnostics_dir(),
        "multi_resolve_duplicate",
        &format_compile_source(&source),
    );
}

#[test]
fn golden_return_local_str() {
    let path = diagnostics_dir().join("return_local_str.phx");
    assert_golden(
        &diagnostics_dir(),
        "return_local_str",
        &format_check_file(&path),
    );
}

#[test]
fn golden_pow_unsupported() {
    let path = diagnostics_dir().join("pow_unsupported.phx");
    assert_golden(
        &diagnostics_dir(),
        "pow_unsupported",
        &format_check_file(&path),
    );
}

#[test]
fn golden_unique_ptr_use_after_move() {
    let path = cli_fixture("unique_ptr_use_after_move.phx");
    assert_golden(
        &diagnostics_dir(),
        "unique_ptr_use_after_move",
        &format_check_file(&path),
    );
}

#[test]
fn golden_trait_impl_incomplete() {
    let path = cli_fixture("trait_impl_incomplete.phx");
    assert_golden(
        &diagnostics_dir(),
        "trait_impl_incomplete",
        &format_check_file(&path),
    );
}

#[test]
fn golden_extern_unsafe() {
    let path = cli_fixture("extern_unsafe.phx");
    assert_golden(
        &diagnostics_dir(),
        "extern_unsafe",
        &format_check_file(&path),
    );
}

#[test]
fn golden_invalid_utf8_byte_as_str() {
    let path = cli_fixture("invalid_utf8_byte_as_str.phx");
    assert_golden(
        &diagnostics_dir(),
        "invalid_utf8_byte_as_str",
        &format_check_file(&path),
    );
}

#[test]
fn golden_match_unreachable_arm() {
    let path = cli_fixture("match_unreachable_arm.phx");
    assert_golden(
        &diagnostics_dir(),
        "match_unreachable_arm",
        &format_check_file(&path),
    );
}

#[test]
fn golden_try_ok_mismatch() {
    let path = phx_test::cli_project_main("std_try_ok_mismatch");
    assert_golden(
        &diagnostics_dir(),
        "try_ok_mismatch",
        &format_check_file(&path),
    );
}

#[test]
fn golden_try_from_missing() {
    let path = phx_test::cli_project_main("std_try_from_missing");
    assert_golden(
        &diagnostics_dir(),
        "try_from_missing",
        &format_check_file(&path),
    );
}

#[test]
fn golden_discarded_std_result() {
    let path = phx_test::cli_project_main("lint_std_result_discard");
    assert_golden(
        &diagnostics_dir(),
        "discarded_std_result",
        &format_check_file(&path),
    );
}

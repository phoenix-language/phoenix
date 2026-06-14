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

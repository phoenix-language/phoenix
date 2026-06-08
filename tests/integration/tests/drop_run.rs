//! V0-054: scope-end drop glue smoke.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_test::{compile_fixture, run_fixture_smoke};

#[test]
fn drop_fixture_compiles_and_verifies() {
    let _ = compile_fixture("drop.phx");
}

#[test]
fn drop_fixture_runs_without_runtime_error() {
    run_fixture_smoke("drop.phx");
}

#[test]
fn drop_double_rejected() {
    check_fixture_fails("drop_double.phx", "moved");
}

#[test]
fn drop_use_after_rejected() {
    check_fixture_fails("drop_use_after.phx", "moved");
}

fn check_fixture_fails(name: &str, needle: &str) {
    let path = phx_test::cli_fixture(name);
    phx_test::fixtures::assert_fixture_exists(&path);
    let err = phx_compiler::check_file(&path).expect_err("expected type error");
    let msg = format!("{err}");
    assert!(
        msg.contains(needle),
        "expected diagnostic containing `{needle}`, got: {msg}"
    );
}

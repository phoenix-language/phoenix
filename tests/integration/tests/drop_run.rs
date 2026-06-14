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
fn drop_double_user_drop_compiles() {
    // PHX-026: user-defined `Drop` homonyms do not trigger std drop/move tracking.
    let _ = compile_fixture("drop_double.phx");
}

#[test]
fn drop_use_after_user_drop_compiles() {
    let _ = compile_fixture("drop_use_after.phx");
}

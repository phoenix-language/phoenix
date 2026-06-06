//! In-process smoke tests: compile → verify → run on positive CLI fixtures.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_test::{SMOKE_FIXTURES, cli_modules_dir, compile_fixture_module, run_fixture_smoke};
use phx_vm::run;

#[test]
fn positive_fixtures_run_without_panic() {
    for name in SMOKE_FIXTURES {
        run_fixture_smoke(name);
    }
}

#[test]
fn modules_main_runs_without_panic() {
    let root = cli_modules_dir();
    let module = compile_fixture_module("main.phx", &root);
    run(&module).expect("run modules/main.phx");
}

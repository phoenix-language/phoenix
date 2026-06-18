//! In-process smoke tests: compile → verify → run on embedded programs.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_test::{SMOKE_PROGRAMS, compile_module_tree, modules_main_tree, run_smoke_program};
use phx_vm::run;

#[test]
fn positive_fixtures_run_without_panic() {
    for program in SMOKE_PROGRAMS {
        run_smoke_program(program);
    }
}

#[test]
fn modules_main_runs_without_panic() {
    let module = compile_module_tree(modules_main_tree());
    let verified = phx_bytecode::verify(&module).expect("verify");
    run(verified).expect("run modules/main.phx");
}

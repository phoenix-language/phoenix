//! End-to-end: compile match fixtures, verify, run on VM.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use phx_bytecode::verify;
use phx_compiler::compile_to_module;
use phx_vm::run;

fn run_fixture(name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../cli/fixtures/{name}"));
    let module = compile_to_module(&path).unwrap_or_else(|e| panic!("compile {name}: {e}"));
    verify(&module).expect("verify bytecode");
    run(&module).expect("run main");
}

#[test]
fn run_match_int_bytecode_without_panic() {
    run_fixture("match_int.phx");
}

#[test]
fn run_match_bool_bytecode_without_panic() {
    run_fixture("match_bool.phx");
}

#[test]
fn run_logical_bytecode_without_panic() {
    run_fixture("logical.phx");
}

#[test]
fn run_continue_in_if_bytecode_without_panic() {
    run_fixture("continue_in_if.phx");
}

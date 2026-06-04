//! End-to-end: compile aggregate fixtures, verify, run on VM.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use phx_bytecode::verify;
use phx_compiler::compile_to_module;
use phx_vm::run;

fn run_fixture(name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../cli/fixtures")
        .join(name);
    let module = compile_to_module(&path).unwrap_or_else(|e| panic!("compile {name}: {e}"));
    verify(&module).unwrap_or_else(|e| panic!("verify {name}: {e}"));
    run(&module).unwrap_or_else(|e| panic!("run {name}: {e}"));
}

#[test]
fn run_struct_point_without_panic() {
    run_fixture("struct_point.phx");
}

#[test]
fn run_struct_assign_without_panic() {
    run_fixture("struct_assign.phx");
}

#[test]
fn run_enum_match_without_panic() {
    run_fixture("enum_match.phx");
}

#[test]
fn run_enum_match_struct_without_panic() {
    run_fixture("enum_match_struct.phx");
}

#[test]
fn run_struct_method_without_panic() {
    run_fixture("struct_method.phx");
}

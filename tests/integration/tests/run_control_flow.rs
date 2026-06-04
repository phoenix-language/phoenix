//! End-to-end: compile `control_flow.phx`, verify, run on VM.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use phx_bytecode::verify;
use phx_compiler::compile_to_module;
use phx_vm::run;

#[test]
fn run_control_flow_bytecode_without_panic() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/fixtures/control_flow.phx");
    let module = compile_to_module(&path).unwrap_or_else(|e| panic!("compile: {e}"));
    verify(&module).expect("verify bytecode");
    run(&module).expect("run main");
}

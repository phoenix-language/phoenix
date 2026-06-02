//! End-to-end: compile sample.phx, codegen, verify, run on VM.

use std::path::Path;

use phx_bytecode::verify;
use phx_compiler::compile_to_module;
use phx_vm::run;

#[test]
fn run_sample_bytecode_without_panic() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/fixtures/sample.phx");
    let module = compile_to_module(&path).unwrap_or_else(|e| panic!("compile: {e}"));
    verify(&module).expect("verify bytecode");
    run(&module).expect("run main");
}

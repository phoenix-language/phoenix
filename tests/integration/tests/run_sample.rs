//! End-to-end: compile sample.phx, codegen, verify, run on VM.

use std::path::Path;

use phx_bytecode::verify;
use phx_compiler::{codegen, compile_source, lower};
use phx_vm::run;

#[test]
fn run_sample_bytecode_without_panic() {
    let source = include_str!("../../../tests/cli/fixtures/sample.phx");
    let unit = compile_source(source, Some(Path::new("sample.phx")))
        .unwrap_or_else(|e| panic!("compile: {e}"));
    let module = codegen(&lower(&unit.typed));
    verify(&module).expect("verify bytecode");
    run(&module).expect("run main");
}

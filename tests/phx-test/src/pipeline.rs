//! Compile → verify → run pipelines for single-file and multi-file fixtures.

use std::path::Path;

use phx_bytecode::{BytecodeModule, verify};
use phx_compiler::{
    check_file, check_file_with_module_path, compile_to_module, compile_to_module_with_module_path,
};
use phx_vm::{VmRunCapture, run, run_captured};

use crate::fixtures::{assert_fixture_exists, cli_fixture};

/// Compile a single-file CLI fixture and verify bytecode.
pub fn compile_fixture(name: &str) -> BytecodeModule {
    let path = cli_fixture(name);
    assert_fixture_exists(&path);
    let module = compile_to_module(&path).unwrap_or_else(|e| panic!("compile {name}: {e}"));
    verify(&module).unwrap_or_else(|e| panic!("verify {name}: {e}"));
    module
}

/// Compile a multi-file entry with an explicit module root and verify bytecode.
pub fn compile_fixture_module(entry: &str, module_root: &Path) -> BytecodeModule {
    let path = module_root.join(entry);
    let label = format!("{}/{}", module_root.display(), entry);
    let module = compile_to_module_with_module_path(&path, module_root)
        .unwrap_or_else(|e| panic!("compile {label}: {e}"));
    verify(&module).unwrap_or_else(|e| panic!("verify {label}: {e}"));
    module
}

/// Type-check a single-file CLI fixture (parse + resolve + typeck only).
pub fn check_fixture_ok(name: &str) {
    let path = cli_fixture(name);
    assert_fixture_exists(&path);
    check_file(&path).unwrap_or_else(|e| panic!("check {name}: {e}"));
}

/// Type-check a multi-file entry with an explicit module root.
pub fn check_fixture_module_ok(entry: &str, module_root: &Path) {
    let path = module_root.join(entry);
    let label = format!("{}/{}", module_root.display(), entry);
    check_file_with_module_path(&path, module_root)
        .unwrap_or_else(|e| panic!("check {label}: {e}"));
}

/// Compile, verify, and run a fixture without inspecting VM output.
pub fn run_fixture_smoke(name: &str) {
    let module = compile_fixture(name);
    run(&module).unwrap_or_else(|e| panic!("run {name}: {e}"));
}

/// Compile, verify, and run a fixture returning captured `main` locals.
pub fn run_fixture_captured(name: &str) -> VmRunCapture {
    let module = compile_fixture(name);
    run_captured(&module).unwrap_or_else(|e| panic!("run {name}: {e}"))
}

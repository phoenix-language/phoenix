//! Compile → verify → run pipelines for embedded and legacy fixtures.

use std::path::Path;

use phx_bytecode::{BytecodeModule, verify};
use phx_compiler::{
    check_file, check_file_with_module_path, compile_to_module, compile_to_module_with_module_path,
};
use phx_programs::{ModuleTree, ProjectSpec, SingleFile, SmokeProgram};
use phx_vm::{VmRunCapture, run, run_captured};

use crate::workspace::{DEFAULT_RUN_TIMEOUT, TempWorkspace, run_with_timeout};

/// Compile a single-file embedded program and verify bytecode.
pub fn compile_program(program: &SingleFile) -> BytecodeModule {
    let ws = TempWorkspace::new(program.name);
    let path = ws.write_single(program);
    let module = compile_path(&path, program.name);
    std::mem::forget(ws);
    module
}

/// Compile a smoke program and verify bytecode.
pub fn compile_smoke(program: &SmokeProgram) -> BytecodeModule {
    let single = SingleFile {
        name: program.name,
        source: program.source,
    };
    compile_program(&single)
}

/// Compile a multi-file module tree and verify bytecode.
pub fn compile_module_tree(tree: &ModuleTree) -> BytecodeModule {
    let ws = TempWorkspace::new(tree.name);
    let entry = ws.write_module_tree(tree);
    let module_root = ws.root().join("tests/cli/fixtures/modules");
    let label = format!("{}/{}", tree.name, tree.entry);
    let module = compile_to_module_with_module_path(&entry, &module_root)
        .unwrap_or_else(|e| panic!("compile {label}: {e}"));
    verify(&module).unwrap_or_else(|e| panic!("verify {label}: {e}"));
    std::mem::forget(ws);
    module
}

/// Type-check an embedded single-file program.
pub fn check_program_ok(program: &SingleFile) {
    let ws = TempWorkspace::new(program.name);
    let path = ws.write_single(program);
    check_file(&path).unwrap_or_else(|e| panic!("check {}: {e}", program.name));
}

/// Type-check a multi-file module tree entry.
pub fn check_module_tree_ok(tree: &ModuleTree) {
    let ws = TempWorkspace::new(tree.name);
    let entry = ws.write_module_tree(tree);
    let module_root = ws.root().join("tests/cli/fixtures/modules");
    let label = format!("{}/{}", tree.name, tree.entry);
    check_file_with_module_path(&entry, &module_root)
        .unwrap_or_else(|e| panic!("check {label}: {e}"));
    std::mem::forget(ws);
}

fn compile_path(path: &Path, label: &str) -> BytecodeModule {
    let module = compile_to_module(path).unwrap_or_else(|e| panic!("compile {label}: {e}"));
    verify(&module).unwrap_or_else(|e| panic!("verify {label}: {e}"));
    module
}

/// Compile a single-file program by legacy fixture name and verify bytecode.
pub fn compile_fixture(name: &str) -> BytecodeModule {
    compile_program(crate::programs::lookup_single(name))
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

/// Type-check a single-file program by legacy fixture name.
pub fn check_fixture_ok(name: &str) {
    check_program_ok(crate::programs::lookup_single(name));
}

/// Type-check a multi-file entry with an explicit module root.
pub fn check_fixture_module_ok(entry: &str, module_root: &Path) {
    let path = module_root.join(entry);
    let label = format!("{}/{}", module_root.display(), entry);
    check_file_with_module_path(&path, module_root)
        .unwrap_or_else(|e| panic!("check {label}: {e}"));
}

/// Compile, verify, and run an embedded smoke program without inspecting VM output.
pub fn run_smoke_program(program: &SmokeProgram) {
    let single = SingleFile {
        name: program.name,
        source: program.source,
    };
    run_program_smoke(&single);
}

/// Compile, verify, and run an embedded single-file program without inspecting VM output.
pub fn run_program_smoke(program: &SingleFile) {
    let name = program.name;
    let source = program.source;
    let label = name.to_owned();
    let timeout_label = label.clone();
    run_with_timeout(&timeout_label, DEFAULT_RUN_TIMEOUT, move || {
        let ws = TempWorkspace::new(name);
        let path = ws.write_single(&SingleFile { name, source });
        let module = compile_path(&path, &label);
        let verified = verify(&module).unwrap_or_else(|e| panic!("verify {label}: {e}"));
        run(verified).unwrap_or_else(|e| panic!("run {label}: {e}"));
    });
}

/// Compile, verify, and run a fixture without inspecting VM output.
pub fn run_fixture_smoke(name: &str) {
    run_program_smoke(crate::programs::lookup_single(name));
}

/// Compile, verify, and run an embedded program returning captured `main` locals.
pub fn run_program_captured(program: &SingleFile) -> VmRunCapture {
    let name = program.name;
    let source = program.source;
    let label = name.to_owned();
    let timeout_label = label.clone();
    run_with_timeout(&timeout_label, DEFAULT_RUN_TIMEOUT, move || {
        let ws = TempWorkspace::new(name);
        let path = ws.write_single(&SingleFile { name, source });
        let module = compile_path(&path, &label);
        let verified = verify(&module).unwrap_or_else(|e| panic!("verify {label}: {e}"));
        run_captured(verified).unwrap_or_else(|e| panic!("run {label}: {e}"))
    })
}

/// Compile, verify, and run a fixture returning captured `main` locals.
pub fn run_fixture_captured(name: &str) -> VmRunCapture {
    run_program_captured(crate::programs::lookup_single(name))
}

/// Entry path for a materialized project (caller holds `TempWorkspace`).
pub fn project_entry_path(
    ws: &TempWorkspace,
    spec: &ProjectSpec,
    entry: &str,
) -> std::path::PathBuf {
    ws.write_project(spec);
    ws.root().join(entry)
}

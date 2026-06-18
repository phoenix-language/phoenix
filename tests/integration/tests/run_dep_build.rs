//! Path dependency build populates `build/deps/`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_bytecode::{Instruction, Opcode, verify};
use phx_compiler::BuildOptions;
use phx_compiler::{
    BuildLayout, ProgramLoadContext, PxiFile, build_project, load_program_with_context,
    resolve_loaded_program, unstable::type_check,
};
use phx_diagnostics::DiagnosticBag;
use phx_test::{build_cli_project, cli_project, discover_cli_project};
use phx_vm::run;

use std::path::PathBuf;

fn build_app_dep() -> PathBuf {
    let root = cli_project("app_dep");
    let config = discover_cli_project(&root);
    build_cli_project(&config, BuildOptions::force(true));
    root
}

#[test]
fn path_dependency_artifacts() {
    let root = build_app_dep();
    let dep_lib = root.join("build/deps/math/lib/math.phx0");
    assert!(
        dep_lib.is_file(),
        "expected dependency lib at {}",
        dep_lib.display()
    );
    let dep_pxi = root.join("build/deps/math/pxi/math.pxi");
    assert!(
        dep_pxi.is_file(),
        "expected dependency interface at {}",
        dep_pxi.display()
    );
    let app_bin = root.join("build/bin/app_dep.phx0");
    assert!(app_bin.is_file());
}

#[test]
fn path_dep_pxi_seeds_import_types() {
    let root = build_app_dep();
    let config = discover_cli_project(&root);

    let entry = config.default_entry_file();
    let ctx = ProgramLoadContext::from_config(&config);
    let layout = BuildLayout::new(&config);
    let mut bag = DiagnosticBag::new();
    let loaded =
        load_program_with_context(&entry, &ctx, Some(&layout), &mut bag).expect("load program");
    assert!(!bag.has_errors(), "load errors: {bag}");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    assert!(
        !resolved.import_types.is_empty(),
        "math::add should get types from build/deps/math/pxi"
    );
    type_check(resolved).expect("typeck with dep pxi types");
}

#[test]
fn path_dep_linked_binary_runs() {
    let root = build_app_dep();
    let bytes = std::fs::read(root.join("build/bin/app_dep.phx0")).expect("read linked bin");
    let module = phx_bytecode::BytecodeModule::decode(&bytes).expect("decode");
    let verified = verify(&module).expect("verify linked bin");

    run(verified).expect("run cross-package binary");
}

#[test]
fn path_dep_links_prebuilt_objects_not_workspace_dep_codegen() {
    let root = build_app_dep();
    let ws_math_phx0 = root.join("build/phx0/math.phx0");
    let ws_math_dir = root.join("build/phx0/math");
    assert!(
        !ws_math_phx0.is_file() && !ws_math_dir.is_dir(),
        "consumer build must not codegen dependency modules under workspace build/phx0"
    );
    let dep_phx0 = root.join("build/deps/math/phx0/math.phx0");
    assert!(dep_phx0.is_file(), "expected {}", dep_phx0.display());
}

#[test]
fn path_dep_call_targets_dependency_function_id() {
    let root = build_app_dep();
    let dep_pxi_path = root.join("build/deps/math/pxi/math.pxi");
    let dep_pxi = PxiFile::read_from_path(&dep_pxi_path).expect("pxi");
    let add_export = dep_pxi
        .exports
        .iter()
        .find(|e| e.name == "add")
        .expect("add export");
    let dep_fn_id = add_export.function_id.expect("function_id in dep pxi");

    let bytes = std::fs::read(root.join("build/bin/app_dep.phx0")).expect("read bin");
    let module = phx_bytecode::BytecodeModule::decode(&bytes).expect("decode");
    let mut saw_call = false;
    let mut pos = 0usize;
    while pos < module.code.len() {
        let Ok((inst, next)) = Instruction::decode_at(&module.code, pos) else {
            break;
        };
        if inst.opcode == Opcode::Call && inst.operands.first() == Some(&dep_fn_id) {
            saw_call = true;
        }
        pos = next;
    }
    assert!(
        saw_call,
        "main should Call dependency fn id {dep_fn_id} from linked image"
    );
}

#[test]
fn path_dep_generic_call_targets_mangled_export() {
    let root = build_app_dep();
    let dep_pxi_path = root.join("build/deps/math/pxi/math.pxi");
    let dep_pxi = PxiFile::read_from_path(&dep_pxi_path).expect("pxi");
    let id_export = dep_pxi
        .exports
        .iter()
        .find(|e| e.name.starts_with("id$"))
        .expect("mangled id export in dep pxi");
    let dep_fn_id = id_export
        .function_id
        .expect("function_id on mangled export");

    let bytes = std::fs::read(root.join("build/bin/app_dep.phx0")).expect("read bin");
    let module = phx_bytecode::BytecodeModule::decode(&bytes).expect("decode");
    let mut saw_call = false;
    let mut pos = 0usize;
    while pos < module.code.len() {
        let Ok((inst, next)) = Instruction::decode_at(&module.code, pos) else {
            break;
        };
        if inst.opcode == Opcode::Call && inst.operands.first() == Some(&dep_fn_id) {
            saw_call = true;
        }
        pos = next;
    }
    assert!(
        saw_call,
        "main should Call mangled generic export fn id {dep_fn_id}"
    );
}

#[test]
fn path_dep_std_smoke_builds_and_runs() {
    let root = cli_project("std_smoke");
    let config = discover_cli_project(&root);
    build_cli_project(&config, BuildOptions::force(true));
    let dep_lib = root.join("build/deps/std/lib/std.phx0");
    assert!(
        dep_lib.is_file(),
        "expected dependency lib at {}",
        dep_lib.display()
    );
    let bytes = std::fs::read(root.join("build/bin/std_smoke.phx0")).expect("read bin");
    let module = phx_bytecode::BytecodeModule::decode(&bytes).expect("decode");
    let verified = verify(&module).expect("verify linked bin");

    run(verified).expect("run std_smoke binary");
}

#[test]
fn bundle_std_disabled_does_not_link_std() {
    let root = cli_project("no_bundle_std");
    let config = discover_cli_project(&root);
    assert!(
        !config.dependencies.contains_key("std"),
        "bundle_std = false must not inject std dependency"
    );
    let err = build_project(&config, None, BuildOptions::force(true))
        .expect_err("Option without std should fail to build");
    let msg = err.to_string();
    assert!(
        msg.contains("Option")
            || msg.to_ascii_lowercase().contains("unresolved")
            || msg.to_ascii_lowercase().contains("unknown"),
        "expected resolve/type error for Option, got: {msg}"
    );
    assert!(
        !root.join("build/deps/std").is_dir(),
        "std must not be built when bundle_std = false"
    );
}

//! Integration tests for V0-060 std error module.

#![allow(clippy::expect_used)]

use std::path::PathBuf;

use phx_compiler::TryFailureMode;
use phx_compiler::{
    BuildLayout, BuildOptions, IrInst, ProjectConfig, build_project, load_program_with_context,
    lower, resolve_loaded_program, type_check,
};
use phx_diagnostics::DiagnosticBag;
use phx_test::fixture_fs_lock;

fn std_errors_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/std_errors")
}

fn std_lib_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../std")
}

#[test]
fn std_lib_builds_with_error_modules() {
    let _lock = fixture_fs_lock();
    let root = std_lib_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load std");
    build_project(&config, None, BuildOptions::force(true)).expect("build std with error modules");
}

#[test]
fn bundled_std_errors_builds() {
    let _lock = fixture_fs_lock();
    let root = std_errors_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    build_project(&config, None, BuildOptions::force(true)).expect("build std_errors");
}

#[test]
fn std_errors_records_convert_err_try_site() {
    let _lock = fixture_fs_lock();
    let root = std_errors_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag)
        .expect("load std_errors program");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    let typed = type_check(&resolved).expect("typecheck");
    assert!(
        typed
            .try_sites
            .values()
            .any(|m| { matches!(m.failure_mode, TryFailureMode::ConvertErr { .. }) }),
        "expected ConvertErr try site for read_bytes()?"
    );
}

#[test]
fn std_errors_read_config_lowers_from_on_err_path() {
    let _lock = fixture_fs_lock();
    let root = std_errors_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag)
        .expect("load std_errors program");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    let typed = type_check(&resolved).expect("typecheck");
    let ir = lower(&typed).expect("lower");
    let interner = &typed.resolved.interner;
    let read_config = ir.functions.iter().find(|f| {
        typed
            .resolved
            .defs
            .get(f.def.index() as usize)
            .is_some_and(|d| interner.resolve(d.name) == "read_config")
    });
    let Some(f) = read_config else {
        panic!("read_config function not found in IR");
    };
    let insts: Vec<_> = f.blocks.iter().flat_map(|b| &b.insts).collect();
    let has_call = insts.iter().any(|i| matches!(i, IrInst::Call { .. }));
    let has_make_enum = insts.iter().any(|i| matches!(i, IrInst::MakeEnum { .. }));
    assert!(
        has_call,
        "read_config should lower Call to From::from on ? failure path"
    );
    assert!(
        has_make_enum,
        "read_config should lower MakeEnum for converted Err return"
    );
}

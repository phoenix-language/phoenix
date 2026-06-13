//! Integration tests for V0-060 std error module.

#![allow(clippy::expect_used)]

use std::path::PathBuf;

use phx_compiler::TryFailureMode;
use phx_compiler::{
    BuildLayout, BuildOptions, ProjectConfig, build_project, load_program_with_context,
    resolve_loaded_program, type_check,
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
fn std_lib_builds_with_core_error_trait() {
    let _lock = fixture_fs_lock();
    let root = std_lib_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load std");
    build_project(&config, None, BuildOptions::force(true))
        .expect("build std with core::error trait");
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
fn std_errors_records_try_site() {
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
    let typed = type_check(resolved).expect("typecheck");
    assert!(
        typed
            .try_sites
            .values()
            .any(|m| { matches!(m.failure_mode, TryFailureMode::ReturnScrutinee) }),
        "expected ReturnScrutinee try site for read_bytes()?"
    );
}

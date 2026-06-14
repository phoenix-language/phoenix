//! V0-061 `mod` declarations, barrel reexports, and visibility.

#![allow(clippy::expect_used)]

use std::path::PathBuf;

use phx_compiler::{
    BuildLayout, BuildOptions, ProjectConfig, load_program_with_context, resolve_loaded_program,
    unstable::type_check,
};
use phx_diagnostics::DiagnosticBag;
use phx_test::fixture_fs_lock;

fn fixture_root(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures")
        .join(name)
}

fn load_project(name: &str) -> Result<phx_compiler::unstable::ResolvedProgram, DiagnosticBag> {
    let root = fixture_root(name);
    let config = ProjectConfig::load(&root).expect("load project");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag)
        .expect("load should succeed");
    resolve_loaded_program(loaded)
}

#[test]
fn bin_barrel_reexport_resolves() {
    let _lock = fixture_fs_lock();
    let root = fixture_root("modules_bin_barrel");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let resolved = load_project("modules_bin_barrel").expect("resolve barrel project");
    type_check(resolved).expect("typecheck barrel import util::add");
}

#[test]
fn missing_module_entry_fails_load() {
    let _lock = fixture_fs_lock();
    let root = fixture_root("modules_missing_mod");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, None, &mut bag);
    assert!(loaded.is_none(), "expected missing mod.phx to fail load");
    let msg = bag.to_string();
    assert!(
        msg.contains("E1021")
            || msg.contains("missing module entry")
            || msg.contains("no `mod.phx`")
            || msg.contains("module not found"),
        "got:\n{msg}"
    );
}

#[test]
fn orphan_file_fails_load() {
    let _lock = fixture_fs_lock();
    let root = fixture_root("modules_orphan_file");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag);
    assert!(loaded.is_none(), "expected orphan file to fail load");
    let msg = bag.to_string();
    assert!(
        msg.contains("E1019") || msg.contains("orphan"),
        "got:\n{msg}"
    );
}

#[test]
fn std_error_import_path_flattened() {
    let _lock = fixture_fs_lock();
    let resolved = load_project("std_errors").expect("resolve std_errors");
    type_check(resolved).expect("typecheck std::core::error::Error imports");
}

#[test]
fn std_iter_for_in_has_plan() {
    let _lock = fixture_fs_lock();
    let root = fixture_root("std_iter");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let resolved = load_project("std_iter").expect("resolve std_iter");
    let typed = type_check(resolved).expect("typecheck std_iter");
    let main_layout = typed
        .functions
        .iter()
        .find(|f| {
            typed
                .resolved
                .defs
                .get(f.def.index() as usize)
                .is_some_and(|d| typed.resolved.interner.resolves_to(d.name, "main"))
        })
        .expect("main layout");
    assert_eq!(
        main_layout.for_in_plans.len(),
        1,
        "expected one ForInPlan on main"
    );
}

#[test]
fn modules_bin_barrel_builds() {
    let _lock = fixture_fs_lock();
    let root = fixture_root("modules_bin_barrel");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    phx_compiler::build_project(&config, None, BuildOptions::force(true))
        .expect("build modules_bin_barrel");
}

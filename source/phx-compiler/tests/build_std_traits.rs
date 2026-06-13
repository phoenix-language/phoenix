//! Integration tests for V0-043 std core traits.

#![allow(clippy::expect_used)]

use std::path::PathBuf;

use phx_compiler::{
    BuildLayout, BuildOptions, ProjectConfig, build_project, load_program_with_context,
    resolve_loaded_program, unstable::type_check,
};
use phx_diagnostics::DiagnosticBag;
use phx_test::fixture_fs_lock;

fn std_traits_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/std_traits")
}

#[test]
fn bundled_std_traits_builds() {
    let _lock = fixture_fs_lock();
    let root = std_traits_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    build_project(&config, None, BuildOptions::force(true)).expect("build std_traits");
}

#[test]
fn std_traits_records_primitive_method_sites() {
    let _lock = fixture_fs_lock();
    let root = std_traits_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag)
        .expect("load std_traits program");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    let typed = type_check(resolved).expect("typecheck");
    assert!(
        !typed.primitive_method_sites.is_empty(),
        "expected primitive eq/clone method sites"
    );
    assert!(
        !typed.primitive_method_sites.is_empty(),
        "expected primitive eq/clone method sites"
    );
}

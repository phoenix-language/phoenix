//! Integration tests for V0-044 prelude.

#![allow(clippy::expect_used)]

use std::path::PathBuf;

use phx_compiler::{
    BuildLayout, BuildOptions, ProjectConfig, build_project, load_program_with_context,
    resolve_loaded_program, unstable::type_check,
};
use phx_diagnostics::DiagnosticBag;
use phx_test::fixture_fs_lock;

fn std_prelude_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/std_prelude")
}

#[test]
fn bundled_std_prelude_builds() {
    let _lock = fixture_fs_lock();
    let root = std_prelude_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded =
        load_program_with_context(&config.default_entry_file(), &ctx, Some(&layout), &mut bag)
            .expect("load");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    let typed = type_check(resolved).expect("typecheck");
    let entry = typed.entry.expect("main entry");
    assert!(
        typed
            .resolved
            .interner
            .resolves_to(typed.resolved.defs[entry.index() as usize].name, "main")
    );
    build_project(&config, None, BuildOptions::force(true)).expect("build std_prelude");
}

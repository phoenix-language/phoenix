//! Integration tests for V0-042 `?` sugar with bundled std.

#![allow(clippy::expect_used)]

use std::path::PathBuf;

use phx_compiler::{
    BuildLayout, BuildOptions, IrInst, ProjectConfig, build_project, load_program_with_context,
    lower, resolve_loaded_program, type_check,
};
use phx_diagnostics::DiagnosticBag;
use phx_test::fixture_fs_lock;

fn std_try_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/std_try")
}

#[test]
fn bundled_std_try_builds() {
    let _lock = fixture_fs_lock();
    let root = std_try_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    build_project(&config, None, BuildOptions::force(true)).expect("build std_try");
}

#[test]
fn std_try_lowers_match_tag_for_question_mark() {
    let _lock = fixture_fs_lock();
    let root = std_try_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag)
        .expect("load std_try program");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    let typed = type_check(&resolved).expect("typecheck");
    assert!(
        !typed.try_sites.is_empty(),
        "expected try_sites for read_bytes(path)?"
    );
    let ir = lower(&typed).expect("lower");
    let has_match_tag = ir.functions.iter().any(|f| {
        f.blocks
            .iter()
            .any(|b| b.insts.iter().any(|i| matches!(i, IrInst::MatchTag { .. })))
    });
    assert!(has_match_tag, "expected MatchTag in IR for ? lowering");
}

#[test]
fn std_try_read_config_ir_has_try_unwrap_sequence() {
    let _lock = fixture_fs_lock();
    let root = std_try_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag)
        .expect("load std_try program");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    let typed = type_check(&resolved).expect("typecheck");
    let ir = lower(&typed).expect("lower");
    let interner = &typed.resolved.interner;
    let read_config = ir.functions.iter().find(|f| {
        typed
            .resolved
            .defs
            .get(f.def.index() as usize)
            .is_some_and(|d| interner.resolves_to(d.name, "read_config"))
    });
    let Some(f) = read_config else {
        return;
    };
    let insts: Vec<_> = f.blocks.iter().flat_map(|b| &b.insts).collect();
    let has_match = insts.iter().any(|i| matches!(i, IrInst::MatchTag { .. }));
    let has_get_field = insts.iter().any(|i| matches!(i, IrInst::GetField { .. }));
    assert!(has_match, "read_config should lower MatchTag for ?");
    assert!(
        has_get_field,
        "read_config should lower GetField after ? match"
    );
    let store_count = insts
        .iter()
        .filter(|i| matches!(i, IrInst::StoreLocal { .. }))
        .count();
    assert!(
        store_count >= 2,
        "expected temp + binding StoreLocal ops, got {store_count}"
    );
}

#[test]
fn std_try_bytecode_type_id_resolved_after_mono() {
    let _lock = fixture_fs_lock();
    let root = std_try_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag)
        .expect("load std_try program");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    let typed = type_check(&resolved).expect("typecheck");
    for meta in typed.try_sites.values() {
        assert!(
            typed
                .layout
                .type_id_for_named(meta.enum_def, &meta.enum_args)
                .is_some(),
            "expected mono layout type id for ? scrutinee"
        );
        assert_eq!(
            meta.success_tag, 0,
            "Result Ok / first variant should be tag 0"
        );
    }
}

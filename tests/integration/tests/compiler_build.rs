//! Compiler API build tests (migrated from phx-compiler/tests/build_*.rs).

#![allow(clippy::expect_used)]

use phx_compiler::unstable::TryFailureMode;
use phx_compiler::{
    BuildLayout, BuildOptions, ProjectConfig, build_project, load_program_with_context,
    resolve_loaded_program,
    unstable::{IrInst, lower, type_check},
};
use phx_diagnostics::DiagnosticBag;
use phx_test::{fixture_fs_lock, require_cli_project, require_std_project};

// from build_dynamic_array_smoke.rs
#[test]
fn dynamic_array_smoke_builds() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("dynamic_array_smoke");
    let config = ProjectConfig::load(&root).expect("load");
    build_project(&config, None, BuildOptions::force(true)).expect("build dynamic_array_smoke");
}

// from build_std_convert.rs
#[test]
fn bundled_std_convert_builds() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_convert");
    let config = ProjectConfig::load(&root).expect("load");
    build_project(&config, None, BuildOptions::force(true)).expect("build std_convert");
}

#[test]
fn std_convert_typechecks() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_convert");
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag)
        .expect("load std_convert program");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    let typed = type_check(resolved).expect("typecheck std_convert");
    assert!(
        !typed.associated_fn_sites.is_empty(),
        "expected associated fn sites for From/TryFrom calls"
    );
}

// from build_std_error.rs
#[test]
fn std_lib_builds_with_core_error_trait() {
    let _lock = fixture_fs_lock();
    let root = require_std_project();
    let config = ProjectConfig::load(&root).expect("load std");
    build_project(&config, None, BuildOptions::force(true))
        .expect("build std with core::error trait");
}

#[test]
fn bundled_std_errors_builds() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_errors");
    let config = ProjectConfig::load(&root).expect("load");
    build_project(&config, None, BuildOptions::force(true)).expect("build std_errors");
}

#[test]
fn std_errors_records_try_site() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_errors");
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

// from build_std_ffi.rs
#[test]
fn std_lib_builds_with_ffi_module() {
    let _lock = fixture_fs_lock();
    let root = require_std_project();
    let config = ProjectConfig::load(&root).expect("load std");
    build_project(&config, None, BuildOptions::force(true)).expect("build std with ffi");
}

// from build_std_prelude.rs
#[test]
fn bundled_std_prelude_builds() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_prelude");
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

// from build_std_smoke.rs
#[test]
fn bundled_std_smoke_builds() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_smoke");
    let config = ProjectConfig::load(&root).expect("load");
    build_project(&config, None, BuildOptions::force(true)).expect("build std_smoke");
}

// from build_std_traits.rs
#[test]
fn bundled_std_traits_builds() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_traits");
    let config = ProjectConfig::load(&root).expect("load");
    build_project(&config, None, BuildOptions::force(true)).expect("build std_traits");
}

#[test]
fn std_traits_records_primitive_method_sites() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_traits");
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

// from build_std_try.rs
#[test]
fn bundled_std_try_builds() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_try");
    let config = ProjectConfig::load(&root).expect("load");
    build_project(&config, None, BuildOptions::force(true)).expect("build std_try");
}

#[test]
fn std_try_lowers_match_tag_for_question_mark() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_try");
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag)
        .expect("load std_try program");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    let typed = type_check(resolved).expect("typecheck");
    assert!(
        !typed.try_sites.is_empty(),
        "expected try_sites for read_bytes(path)?"
    );
    let ir = lower(&typed).expect("lower");
    let has_match_tag = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts
                .iter()
                .any(|s| matches!(&s.inst, IrInst::MatchTag { .. }))
        })
    });
    assert!(has_match_tag, "expected MatchTag in IR for ? lowering");
}

#[test]
fn std_try_read_config_ir_has_try_unwrap_sequence() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_try");
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag)
        .expect("load std_try program");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    let typed = type_check(resolved).expect("typecheck");
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
        panic!("read_config function not found in IR");
    };
    let insts: Vec<_> = f.blocks.iter().flat_map(|b| &b.insts).collect();
    let has_match = insts
        .iter()
        .any(|s| matches!(&s.inst, IrInst::MatchTag { .. }));
    let has_get_field = insts
        .iter()
        .any(|s| matches!(&s.inst, IrInst::GetField { .. }));
    assert!(has_match, "read_config should lower MatchTag for ?");
    assert!(
        has_get_field,
        "read_config should lower GetField after ? match"
    );
    let store_count = insts
        .iter()
        .filter(|s| matches!(&s.inst, IrInst::StoreLocal { .. }))
        .count();
    assert!(
        store_count >= 2,
        "expected temp + binding StoreLocal ops, got {store_count}"
    );
}

#[test]
fn std_try_bytecode_type_id_resolved_after_mono() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_try");
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag)
        .expect("load std_try program");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    let typed = type_check(resolved).expect("typecheck");
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

// from build_std_try_from.rs
#[test]
fn bundled_std_try_from_builds() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_try_from");
    let config = ProjectConfig::load(&root).expect("load");
    build_project(&config, None, BuildOptions::force(true)).expect("build std_try_from");
}

#[test]
fn std_try_from_records_convert_err_try_site() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_try_from");
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag)
        .expect("load std_try_from program");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    let typed = type_check(resolved).expect("typecheck");
    assert!(
        typed
            .try_sites
            .values()
            .any(|m| { matches!(m.failure_mode, TryFailureMode::ConvertErr { .. }) }),
        "expected ConvertErr try site for read_bytes()?"
    );
}

#[test]
fn std_try_from_read_config_lowers_from_on_err_path() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_try_from");
    let config = ProjectConfig::load(&root).expect("load");
    let entry = config.default_entry_file();
    let layout = BuildLayout::new(&config);
    let ctx = phx_compiler::ProgramLoadContext::from_config(&config);
    let mut bag = DiagnosticBag::new();
    let loaded = load_program_with_context(&entry, &ctx, Some(&layout), &mut bag)
        .expect("load std_try_from program");
    let resolved = resolve_loaded_program(loaded).expect("resolve");
    let typed = type_check(resolved).expect("typecheck");
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
        panic!("read_config function not found in IR");
    };
    let insts: Vec<_> = f.blocks.iter().flat_map(|b| &b.insts).collect();
    let has_call = insts.iter().any(|s| matches!(&s.inst, IrInst::Call { .. }));
    let has_make_enum = insts
        .iter()
        .any(|s| matches!(&s.inst, IrInst::MakeEnum { .. }));
    assert!(
        has_call,
        "read_config should lower Call to From::from on ? failure path"
    );
    assert!(
        has_make_enum,
        "read_config should lower MakeEnum for converted Err return"
    );
}

// from build_unique_ptr_smoke.rs
#[test]
fn unique_ptr_smoke_builds() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("unique_ptr_smoke");
    let config = ProjectConfig::load(&root).expect("load");
    build_project(&config, None, BuildOptions::force(true)).expect("build unique_ptr_smoke");
}

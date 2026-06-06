//! Integration tests for [`phx_compiler::compile_source`] (parse + resolve).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_compiler::{CompileError, compile_source_with_module_root};
use phx_diagnostics::ResolveError;
use phx_test::{cli_fixtures_dir, compile_ok, expect_resolve_err};

fn resolve_err(source: &str) -> phx_diagnostics::DiagnosticBag {
    expect_resolve_err(source)
}

#[test]
fn empty_main_compile_ok() {
    compile_ok("main :: () => { };");
}

#[test]
fn main_with_body_compile_ok() {
    compile_ok("main :: () => { const x = 1; };");
}

#[test]
fn missing_main_only_const() {
    let bag = resolve_err("const x = 1;");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e.error, ResolveError::MissingMain { .. }))
    );
}

#[test]
fn duplicate_function() {
    let bag = resolve_err(
        "main :: () => { };
         foo :: () => { };
         foo :: () => { };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, ResolveError::DuplicateDefinition { .. }))
    );
}

#[test]
fn unresolved_ident() {
    let bag = resolve_err("main :: () => { unknown_var; };");
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(&e.error, ResolveError::UnresolvedIdent { .. }))
        .expect("UnresolvedIdent");
    assert!(
        err.error.span().is_some_and(|s| s.start > 0),
        "expected non-zero span for unresolved ident"
    );
}

#[test]
fn duplicate_definition_has_span() {
    let bag = resolve_err(
        "main :: () => { };
         foo :: () => { };
         foo :: () => { };",
    );
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(&e.error, ResolveError::DuplicateDefinition { .. }))
        .expect("DuplicateDefinition");
    assert!(
        err.error.span().is_some_and(|s| s.start > 0),
        "expected non-zero span for duplicate definition"
    );
}

#[test]
fn phase2_continues_after_phase1_error_in_other_module() {
    let root = cli_fixtures_dir().join("modules");
    let entry = root.join("main_bad_import.phx");
    let source = std::fs::read_to_string(&entry).expect("read main_bad_import.phx");
    let err = match compile_source_with_module_root(&source, &entry, &root) {
        Err(CompileError::Resolve { bag, .. }) => bag,
        Err(other) => panic!("expected resolve error, got {other}"),
        Ok(_) => panic!("expected resolve failure from bad_dup"),
    };
    let modules: std::collections::HashSet<_> = err.errors().iter().map(|e| e.module).collect();
    assert!(
        modules.len() >= 2,
        "expected errors from multiple modules, got modules {modules:?}: {err:?}"
    );
    assert!(
        err.errors()
            .iter()
            .any(|e| { matches!(&e.error, ResolveError::DuplicateDefinition { .. }) })
    );
}

#[test]
fn cyclic_import_reports_cycle() {
    let root = cli_fixtures_dir().join("modules");
    let entry = root.join("cycle_a.phx");
    let source = std::fs::read_to_string(&entry).expect("read cycle_a.phx");
    let err = match compile_source_with_module_root(&source, &entry, &root) {
        Err(CompileError::Resolve { bag, .. }) => bag,
        Err(other) => panic!("expected resolve error, got {other}"),
        Ok(_) => panic!("expected cyclic import failure"),
    };
    assert!(
        err.errors().iter().any(|e| {
            matches!(
                &e.error,
                ResolveError::CircularImport {
                    cycle,
                    ..
                } if cycle.contains("cycle_a")
                    && cycle.contains("cycle_b")
                    && cycle.contains("→")
            ) && e.error.span().is_some_and(|s| s.end > s.start)
        }),
        "expected CircularImport with module path trace: {err:?}"
    );
}

#[test]
fn compile_with_module_root_imports_compile_ok() {
    let root = cli_fixtures_dir().join("modules");
    let entry = root.join("main.phx");
    let source = std::fs::read_to_string(&entry).expect("read main.phx");
    compile_source_with_module_root(&source, &entry, &root)
        .unwrap_or_else(|e| panic!("expected ok: {e}"));
}

#[test]
fn compile_with_module_root_list_import_compile_ok() {
    let root = cli_fixtures_dir().join("modules");
    let entry = root.join("main_list.phx");
    let source = std::fs::read_to_string(&entry).expect("read main_list.phx");
    compile_source_with_module_root(&source, &entry, &root)
        .unwrap_or_else(|e| panic!("expected ok: {e}"));
}

#[test]
fn compile_with_module_root_glob_import_compile_ok() {
    let root = cli_fixtures_dir().join("modules");
    let entry = root.join("main_glob.phx");
    let source = std::fs::read_to_string(&entry).expect("read main_glob.phx");
    compile_source_with_module_root(&source, &entry, &root)
        .unwrap_or_else(|e| panic!("expected ok: {e}"));
}

#[test]
fn duplicate_import_in_list_rejected() {
    let root = cli_fixtures_dir().join("modules");
    let entry = root.join("import_dup.phx");
    let source = std::fs::read_to_string(&entry).expect("read import_dup.phx");
    let err = match compile_source_with_module_root(&source, &entry, &root) {
        Err(CompileError::Resolve { bag, .. }) => bag,
        Err(other) => panic!("expected resolve error, got {other}"),
        Ok(_) => panic!("expected duplicate import failure"),
    };
    assert!(
        err.errors()
            .iter()
            .any(|e| matches!(&e.error, ResolveError::DuplicateImport { .. })),
        "expected DuplicateImport: {err:?}"
    );
}

#[test]
fn import_not_supported() {
    let bag = resolve_err("#import std::io; main :: () => { };");
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(&e.error, ResolveError::ImportNotSupported { .. }))
        .expect("ImportNotSupported");
    assert!(
        err.error.to_string().contains("module root"),
        "expected module-root hint, got: {}",
        err.error
    );
}

#[test]
fn main_with_params_invalid() {
    let bag = resolve_err("main :: (x: s32) => { };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, ResolveError::InvalidMainSignature { .. }))
    );
}

#[test]
fn main_non_unit_return_invalid() {
    let bag = resolve_err("main :: () => s32 { 0 };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, ResolveError::InvalidMainSignature { .. }))
    );
}

#[test]
fn ok_ctor_unresolved_until_std() {
    let bag = resolve_err("main :: () => { Ok(1); };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, ResolveError::UnresolvedType { .. }))
    );
}

#[test]
fn result_type_unresolved_until_std() {
    let bag = resolve_err("main :: () => { const x: Result<s32, s32> = Ok(1); };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, ResolveError::UnresolvedType { .. }))
    );
}

#[test]
fn some_ctor_unresolved_until_std() {
    let bag = resolve_err("main :: () => { const x: Option<s32> = Some(1); };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, ResolveError::UnresolvedType { .. }))
    );
}

#[test]
fn err_ctor_unresolved_until_std() {
    let bag = resolve_err("main :: () => { const x: Result<s32, s32> = Err(1); };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, ResolveError::UnresolvedType { .. }))
    );
}

#[test]
fn none_ctor_unresolved_until_std() {
    let bag = resolve_err("main :: () => { const x: Option<s32> = None; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, ResolveError::UnresolvedType { .. }))
    );
}

#[test]
fn generic_param_in_value_position() {
    let bag = resolve_err("bad :: <t> () => s32 { t }; main :: () => { };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| { matches!(&e.error, ResolveError::GenericParamInValue { .. }) }),
        "expected GenericParamInValue: {:?}",
        bag.errors()
    );
}

#[test]
fn duplicate_trait_impl_rejected() {
    let bag = resolve_err(
        "PartialEq :: trait { eq :: (self: Point, other: Point) => bool; };
         Point :: struct { x: s32, y: s32, };
         Point :: impl :: PartialEq {
             eq :: (self: Point, other: Point) => bool { true };
         };
         Point :: impl :: PartialEq {
             eq :: (self: Point, other: Point) => bool { false };
         };
         main :: () => { };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| { matches!(&e.error, ResolveError::DuplicateTraitImpl { .. }) })
    );
}

#[test]
fn lambda_closure_records_upvar() {
    use phx_compiler::resolve;
    use phx_syntax::parse;

    let src = "main :: () => { const x = 1; const _f = () => x; };";
    let sf = parse(src).expect("parse");
    let resolved = resolve(&sf).expect("resolve");
    assert!(!resolved.closures.is_empty(), "expected closure metadata");
    let info = resolved.closures.values().next().expect("closure info");
    assert!(!info.upvars.is_empty(), "expected captured outer binding");
}

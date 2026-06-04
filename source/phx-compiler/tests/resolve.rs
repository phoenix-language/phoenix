//! Integration tests for [`phx_compiler::compile_source`] (parse + resolve).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use phx_compiler::CompileError;
use phx_compiler::{compile_source, compile_source_with_module_root};
use phx_diagnostics::{DiagnosticBag, ResolveError};

fn ok(source: &str) {
    compile_source(source, None).unwrap_or_else(|e| panic!("expected ok: {e}"));
}

fn resolve_err(source: &str) -> DiagnosticBag {
    match compile_source(source, None) {
        Err(CompileError::Resolve { bag, .. }) => bag,
        Err(other) => panic!("expected resolve error, got {other}"),
        Ok(_) => panic!("expected resolve error"),
    }
}

#[test]
fn empty_main_ok() {
    ok("main :: () => { };");
}

#[test]
fn main_with_body_ok() {
    ok("main :: () => { const x = 1; };");
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/modules");
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/modules");
    let entry = root.join("cycle_a.phx");
    let source = std::fs::read_to_string(&entry).expect("read cycle_a.phx");
    let err = match compile_source_with_module_root(&source, &entry, &root) {
        Err(CompileError::Resolve { bag, .. }) => bag,
        Err(other) => panic!("expected resolve error, got {other}"),
        Ok(_) => panic!("expected cyclic import failure"),
    };
    assert!(
        err.errors().iter().any(|e| {
            matches!(&e.error, ResolveError::CircularImport { .. })
                && e.error.span().is_some_and(|s| s.end > s.start)
        }),
        "expected CircularImport with import-site span: {err:?}"
    );
}

#[test]
fn compile_with_module_root_imports_ok() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/modules");
    let entry = root.join("main.phx");
    let source = std::fs::read_to_string(&entry).expect("read main.phx");
    compile_source_with_module_root(&source, &entry, &root)
        .unwrap_or_else(|e| panic!("expected ok: {e}"));
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

//! Integration tests for [`phx_compiler::compile_source`] (parse + resolve).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_compiler::CompileError;
use phx_compiler::compile_source;
use phx_diagnostics::{DiagnosticBag, ResolveError};

fn ok(source: &str) {
    compile_source(source, None).unwrap_or_else(|e| panic!("expected ok: {e}"));
}

fn resolve_err(source: &str) -> DiagnosticBag {
    match compile_source(source, None) {
        Err(CompileError::Resolve(bag)) => bag,
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
            .any(|e| matches!(e, ResolveError::MissingMain))
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
            .any(|e| matches!(e, ResolveError::DuplicateDefinition { .. }))
    );
}

#[test]
fn unresolved_ident() {
    let bag = resolve_err("main :: () => { unknown_var; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, ResolveError::UnresolvedIdent { .. }))
    );
}

#[test]
fn import_not_supported() {
    let bag = resolve_err("#import std::io; main :: () => { };");
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(e, ResolveError::ImportNotSupported { .. }))
        .expect("ImportNotSupported");
    assert!(
        err.to_string().contains("module root"),
        "expected module-root hint, got: {err}"
    );
}

#[test]
fn main_with_params_invalid() {
    let bag = resolve_err("main :: (x: s32) => { };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, ResolveError::InvalidMainSignature { .. }))
    );
}

#[test]
fn main_non_unit_return_invalid() {
    let bag = resolve_err("main :: () => s32 { 0 };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, ResolveError::InvalidMainSignature { .. }))
    );
}

#[test]
fn ok_ctor_unresolved_until_std() {
    let bag = resolve_err("main :: () => { Ok(1); };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, ResolveError::UnresolvedType { .. }))
    );
}

#[test]
fn result_type_unresolved_until_std() {
    let bag = resolve_err("main :: () => { const x: Result<s32, s32> = Ok(1); };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, ResolveError::UnresolvedType { .. }))
    );
}

#[test]
fn some_ctor_unresolved_until_std() {
    let bag = resolve_err("main :: () => { const x: Option<s32> = Some(1); };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, ResolveError::UnresolvedType { .. }))
    );
}

#[test]
fn err_ctor_unresolved_until_std() {
    let bag = resolve_err("main :: () => { const x: Result<s32, s32> = Err(1); };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, ResolveError::UnresolvedType { .. }))
    );
}

#[test]
fn none_ctor_unresolved_until_std() {
    let bag = resolve_err("main :: () => { const x: Option<s32> = None; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, ResolveError::UnresolvedType { .. }))
    );
}

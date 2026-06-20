//! Integration tests for [`phx_compiler::compile_source`] (parse + resolve).

mod support;

use phx_compiler::{CompileError, compile_source_with_module_root, unstable::CompilationUnit};
use phx_diagnostics::ResolveError;
use support::{
    compile_ok, expect_resolve_err, module_entry_source, modules_fixture_root_and_entry, test_ok,
    test_some,
};

fn compile_named_module_tree(tree_name: &str) -> CompilationUnit {
    let (root, entry) = modules_fixture_root_and_entry(tree_name);
    let (_, source) = module_entry_source(tree_name);
    test_ok(
        compile_source_with_module_root(source, &entry, &root),
        &format!("compile {tree_name}"),
    )
}

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
        .find(|e| matches!(&e.error, ResolveError::UnresolvedIdent { .. }));
    let err = test_some(err, "UnresolvedIdent");
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
        .find(|e| matches!(&e.error, ResolveError::DuplicateDefinition { .. }));
    let err = test_some(err, "DuplicateDefinition");
    assert!(
        err.error.span().is_some_and(|s| s.start > 0),
        "expected non-zero span for duplicate definition"
    );
}

#[test]
fn phase2_continues_after_phase1_error_in_other_module() {
    let (root, entry) = modules_fixture_root_and_entry("main_bad_import");
    let (_, source) = module_entry_source("main_bad_import");
    let err = match compile_source_with_module_root(source, &entry, &root) {
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
    let (root, entry) = modules_fixture_root_and_entry("cycle_a");
    let (_, source) = module_entry_source("cycle_a");
    let err = match compile_source_with_module_root(source, &entry, &root) {
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
    compile_named_module_tree("main");
}

#[test]
fn compile_with_module_root_list_import_compile_ok() {
    compile_named_module_tree("main_list");
}

#[test]
fn compile_with_module_root_glob_import_compile_ok() {
    compile_named_module_tree("main_glob");
}

#[test]
fn compile_with_module_root_block_import_compile_ok() {
    compile_named_module_tree("block_import_main");
}

#[test]
fn duplicate_import_in_list_rejected() {
    let (root, entry) = modules_fixture_root_and_entry("import_dup");
    let (_, source) = module_entry_source("import_dup");
    let err = match compile_source_with_module_root(source, &entry, &root) {
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
        .find(|e| matches!(&e.error, ResolveError::ImportNotSupported { .. }));
    let err = test_some(err, "ImportNotSupported");
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
    use phx_compiler::unstable::resolve;
    use phx_syntax::parse;

    let src = "main :: () => { const x = 1; const _f = () => x; };";
    let sf = parse(src);
    assert!(!sf.has_errors(), "parse: {:?}", sf.errors);
    let sf = sf.value;
    let resolved = test_ok(resolve(&sf), "resolve");
    assert!(!resolved.closures.is_empty(), "expected closure metadata");
    let info = test_some(resolved.closures.values().next(), "closure info");
    assert!(!info.upvars.is_empty(), "expected captured outer binding");
}

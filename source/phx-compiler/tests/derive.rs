//! Tests for `#[derive]` expansion.

mod support;

use phx_compiler::{
    expand_derives,
    unstable::{resolve, type_check},
};
use phx_syntax::ast::decl::TopLevelDecl;
use phx_syntax::ast::types::Type;
use phx_syntax::parse;
use support::{test_err, test_ok, test_some};

fn expand_and_count_impls(source: &str) -> usize {
    let parsed = parse(source);
    assert!(!parsed.has_errors(), "parse: {:?}", parsed.errors);
    let mut file = parsed.value;
    test_ok(
        expand_derives(&mut file.program, &mut file.interner),
        "expand",
    );
    file.program
        .items
        .iter()
        .filter(|item| matches!(item.inner.decl, TopLevelDecl::Impl { .. }))
        .count()
}

fn expand_source(source: &str) -> phx_syntax::SourceFile {
    let parsed = parse(source);
    assert!(!parsed.has_errors(), "parse: {:?}", parsed.errors);
    let mut file = parsed.value;
    test_ok(
        expand_derives(&mut file.program, &mut file.interner),
        "expand",
    );
    file
}

#[test]
fn expand_partialeq_adds_impl() {
    let source = "PartialEq :: trait { eq :: (self: &Self, other: &Self) => bool; }; \
                  #[derive(PartialEq)] Point :: struct { x: s32, y: s32 }; main :: () => { };";
    assert_eq!(expand_and_count_impls(source), 1);
}

#[test]
fn expand_enum_partialeq_adds_impl() {
    let source = "PartialEq :: trait { eq :: (self: &Self, other: &Self) => bool; }; \
                  #[derive(PartialEq)] Color :: enum { Red, Green(s32) }; main :: () => { };";
    assert_eq!(expand_and_count_impls(source), 1);
}

#[test]
fn expand_unsupported_trait_errors() {
    let parsed = parse("#[derive(Clone)] Point :: struct { x: s32 }; main :: () => { };");
    assert!(!parsed.has_errors());
    let mut file = parsed.value;
    let err = test_err(
        expand_derives(&mut file.program, &mut file.interner),
        "clone",
    );
    assert!(err.message.contains("unsupported derive trait"));
}

#[test]
fn expand_debug_adds_impl() {
    let source = "Debug :: trait { fmt :: (self: &Self) => [u8; 32]; }; \
                  #[derive(Debug)] Point :: struct { x: s32, y: s32 }; main :: () => { };";
    assert_eq!(expand_and_count_impls(source), 1);
}

#[test]
fn expand_generic_struct_partialeq_adds_bounded_impl() {
    let source = "PartialEq :: trait { eq :: (self: &Self, other: &Self) => bool; }; \
                  #[derive(PartialEq)] Box :: <t> struct { v: t }; main :: () => { };";
    assert_eq!(expand_and_count_impls(source), 1);
    let file = expand_source(source);
    let impl_item = file
        .program
        .items
        .iter()
        .find(|item| matches!(item.inner.decl, TopLevelDecl::Impl { .. }));
    let impl_item = test_some(impl_item, "impl");
    let TopLevelDecl::Impl { generics, .. } = &impl_item.inner.decl else {
        panic!("expected impl");
    };
    let generics = test_some(generics.as_ref(), "generic impl");
    assert_eq!(generics.len(), 1);
    let bounds = test_some(generics[0].bounds.as_ref(), "PartialEq bound");
    assert_eq!(bounds.len(), 1);
}

#[test]
fn expand_generic_enum_partialeq_adds_impl() {
    let source = "PartialEq :: trait { eq :: (self: &Self, other: &Self) => bool; }; \
                  #[derive(PartialEq)] Maybe :: <t> enum { None, Some(t) }; main :: () => { };";
    assert_eq!(expand_and_count_impls(source), 1);
}

#[test]
fn derive_partialeq_typechecks() {
    let source = "PartialEq :: trait { eq :: (self: &Self, other: &Self) => bool; }; \
                  #[derive(PartialEq)] Point :: struct { x: s32, y: s32 }; \
                  main :: () => { const p = Point { x: 1, y: 2 }; const q = Point { x: 1, y: 2 }; \
                  const _: bool = p.eq(&q); };";
    let parsed = parse(source);
    assert!(!parsed.has_errors());
    let mut file = parsed.value;
    test_ok(
        expand_derives(&mut file.program, &mut file.interner),
        "expand",
    );
    let resolved = test_ok(resolve(&file), "resolve");
    test_ok(type_check(resolved), "typecheck");
}

#[test]
fn derive_enum_partialeq_typechecks() {
    let source = "PartialEq :: trait { eq :: (self: &Self, other: &Self) => bool; }; \
                  #[derive(PartialEq)] Shape :: enum { Nil(), Point(s32, s32) }; \
                  main :: () => { const a = Nil(); const b = Nil(); const _: bool = a.eq(&b); };";
    let parsed = parse(source);
    assert!(!parsed.has_errors());
    let mut file = parsed.value;
    test_ok(
        expand_derives(&mut file.program, &mut file.interner),
        "expand",
    );
    let resolved = test_ok(resolve(&file), "resolve");
    test_ok(type_check(resolved), "typecheck");
}

#[test]
fn derive_generic_struct_partialeq_typechecks() {
    let source = "PartialEq :: trait { eq :: (self: &Self, other: &Self) => bool; }; \
                  #[derive(PartialEq)] Box :: <t> struct { v: s32 }; \
                  main :: () => { const a = Box :: <s32> { v: 1 }; const b = Box :: <s32> { v: 1 }; \
                  const _: bool = a.eq(&b); };";
    let parsed = parse(source);
    assert!(!parsed.has_errors());
    let mut file = parsed.value;
    test_ok(
        expand_derives(&mut file.program, &mut file.interner),
        "expand",
    );
    let resolved = test_ok(resolve(&file), "resolve");
    test_ok(type_check(resolved), "typecheck");
}

#[test]
fn derive_generic_enum_partialeq_typechecks() {
    let source = "PartialEq :: trait { eq :: (self: &Self, other: &Self) => bool; }; \
                  #[derive(PartialEq)] Maybe :: <t> enum { None, Other(s32) }; \
                  main :: () => { const a: Maybe<s32> = Other(1); const b: Maybe<s32> = Other(1); \
                  const _: bool = a.eq(&b); };";
    let parsed = parse(source);
    assert!(!parsed.has_errors());
    let mut file = parsed.value;
    test_ok(
        expand_derives(&mut file.program, &mut file.interner),
        "expand",
    );
    let resolved = test_ok(resolve(&file), "resolve");
    test_ok(type_check(resolved), "typecheck");
}

#[test]
fn expand_std_dynamic_array_partialeq_has_eq_method() {
    use phx_syntax::Interner;
    use phx_syntax::ast::decl::ImplMember;
    use phx_syntax::parse_with_interner;

    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../std/src/collections/dynamic_array.phx");
    let source = test_ok(std::fs::read_to_string(&path), "read dynamic_array");
    let mut interner = Interner::new();
    let parsed = parse_with_interner(&source, &mut interner);
    assert!(!parsed.has_errors(), "parse: {:?}", parsed.errors);
    let mut file = parsed.value;
    test_ok(
        expand_derives(&mut file.program, &mut file.interner),
        "expand",
    );
    let mut found = false;
    for item in &file.program.items {
        let TopLevelDecl::Impl {
            type_name,
            trait_,
            members,
            ..
        } = &item.inner.decl
        else {
            continue;
        };
        if !interner.resolves_to(type_name.symbol, "DynamicArray") {
            continue;
        }
        let Some(trait_ty) = trait_ else { continue };
        let Type::Named { name, .. } = &trait_ty.inner else {
            continue;
        };
        if !interner.resolves_to(name.symbol, "PartialEq") {
            continue;
        }
        found = true;
        assert_eq!(members.len(), 1, "expected one method");
        assert!(matches!(&members[0], ImplMember::Method(_)));
    }
    assert!(found, "PartialEq impl not found");
}

#[test]
fn derive_generic_struct_imported_trait_typechecks() {
    use phx_compiler::compile_source_with_module_root;

    let (root, entry) = support::modules_fixture_root_and_entry("derive_import_main");
    let (_, source) = support::module_entry_source("derive_import_main");
    test_ok(
        compile_source_with_module_root(source, &entry, &root),
        "expected ok",
    );
}

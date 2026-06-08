//! Tests for `#derive` expansion.

#![allow(clippy::expect_used)]

use phx_compiler::{expand_derives, resolve, type_check};
use phx_syntax::ast::decl::TopLevelDecl;
use phx_syntax::parse;

fn expand_and_count_impls(source: &str) -> usize {
    let mut file = parse(source).expect("parse");
    expand_derives(&mut file.program, &file.interner).expect("expand");
    file.program
        .items
        .iter()
        .filter(|item| matches!(item.inner.decl, TopLevelDecl::Impl { .. }))
        .count()
}

#[test]
fn expand_partialeq_adds_impl() {
    let source = "PartialEq :: trait { eq :: (self: &Self, other: &Self) => bool; }; \
                  #derive(PartialEq) Point :: struct { x: s32, y: s32 }; main :: () => { };";
    assert_eq!(expand_and_count_impls(source), 1);
}

#[test]
fn expand_enum_partialeq_adds_impl() {
    let source = "PartialEq :: trait { eq :: (self: &Self, other: &Self) => bool; }; \
                  #derive(PartialEq) Color :: enum { Red, Green(s32) }; main :: () => { };";
    assert_eq!(expand_and_count_impls(source), 1);
}

#[test]
fn expand_unsupported_trait_errors() {
    let mut file =
        parse("#derive(Clone) Point :: struct { x: s32 }; main :: () => { };").expect("parse");
    let err = expand_derives(&mut file.program, &file.interner).expect_err("clone");
    assert!(err.message.contains("unsupported derive trait"));
}

#[test]
fn derive_partialeq_typechecks() {
    let source = "PartialEq :: trait { eq :: (self: &Self, other: &Self) => bool; }; \
                  #derive(PartialEq) Point :: struct { x: s32, y: s32 }; \
                  main :: () => { const p = Point { x: 1, y: 2 }; const q = Point { x: 1, y: 2 }; \
                  const _: bool = p.eq(&q); };";
    let mut file = parse(source).expect("parse");
    expand_derives(&mut file.program, &file.interner).expect("expand");
    let resolved = resolve(&file).expect("resolve");
    type_check(&resolved).expect("typecheck");
}

#[test]
fn derive_enum_partialeq_typechecks() {
    let source = "PartialEq :: trait { eq :: (self: &Self, other: &Self) => bool; }; \
                  #derive(PartialEq) Shape :: enum { Nil(), Point(s32, s32) }; \
                  main :: () => { const a = Nil(); const b = Nil(); const _: bool = a.eq(&b); };";
    let mut file = parse(source).expect("parse");
    expand_derives(&mut file.program, &file.interner).expect("expand");
    let resolved = resolve(&file).expect("resolve");
    type_check(&resolved).expect("typecheck");
}

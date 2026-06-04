//! Integration tests for the type-checking pass.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_compiler::CompileError;
use phx_compiler::compile_source;
use phx_diagnostics::{TypeCheckBag, TypeCheckError};

fn ok(source: &str) {
    compile_source(source, None).unwrap_or_else(|e| panic!("expected ok: {e}"));
}

fn typeck_err(source: &str) -> TypeCheckBag {
    match compile_source(source, None) {
        Err(CompileError::TypeCheck(bag)) => bag,
        Err(other) => panic!("expected type-check error, got {other}"),
        Ok(_) => panic!("expected type-check error"),
    }
}

fn has_unsupported(bag: &TypeCheckBag, needle: &str) -> bool {
    bag.errors().iter().any(|e| {
        matches!(e, TypeCheckError::UnsupportedFeature { feature, .. } if feature.contains(needle))
    })
}

#[test]
fn empty_main_ok() {
    ok("main :: () => { };");
}

#[test]
fn while_loop_ok() {
    ok("main :: () => { var i: s32 = 0; while 3 > (i) { i = i + 1; }; };");
}

#[test]
fn loop_break_continue_ok() {
    ok("main :: () => { loop { break; }; loop { continue; }; };");
}

#[test]
fn break_outside_loop() {
    let bag = typeck_err("main :: () => { break; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::LoopControlOutsideLoop { .. }))
    );
}

#[test]
fn continue_outside_loop() {
    let bag = typeck_err("main :: () => { continue; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::LoopControlOutsideLoop { .. }))
    );
}

#[test]
fn const_inference_ok() {
    ok("main :: () => { const x = 1; };");
}

#[test]
fn if_branch_mismatch() {
    let bag = typeck_err("main :: () => { const x: s32 = { if true { 1 } else { false } }; };");
    assert!(bag.errors().iter().any(|e| {
        matches!(
            e,
            TypeCheckError::NonUnifyingBranches { .. } | TypeCheckError::Mismatch { .. }
        )
    }));
}

#[test]
fn question_mark_unsupported_in_mvp() {
    let bag = typeck_err("main :: () => { const _ = 1?; };");
    assert!(has_unsupported(&bag, "`?`"));
}

#[test]
fn use_after_move_error() {
    let source = "Point :: struct { x: s32, y: s32, }; main :: () => { var p: Point = Point { x: 1, y: 2 }; var q: Point = p; const _ = p.x; };";
    let bag = typeck_err(source);
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(e, TypeCheckError::UseAfterMove { .. }))
        .expect("use-after-move");
    if let TypeCheckError::UseAfterMove { name, .. } = err {
        assert_eq!(name, "p");
    }
    let msg = phx_diagnostics::format_typecheck_error(source, err);
    assert!(msg.contains("moved"));
    assert!(msg.contains("note:"));
}

#[test]
fn function_trailing_expr_return_ok() {
    ok("add :: (a: s32, b: s32) => s32 { a + b }; main :: () => { };");
}

#[test]
fn function_return_stmt_ok() {
    ok("f :: () => s32 { return 1; }; main :: () => { };");
}

#[test]
fn function_body_return_mismatch() {
    let bag = typeck_err("f :: () => s32 { true }; main :: () => { };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::Mismatch { .. }))
    );
}

#[test]
fn invalid_cast() {
    let bag = typeck_err("main :: () => { const x = 1 as bool; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::InvalidCast { .. }))
    );
}

#[test]
fn cross_width_cast_ok() {
    ok(
        "main :: () => { const wide: s64 = 100 as s64; const narrow: u8 = 42 as u8; const bump: s64 = narrow as s64; const _ = wide + bump; };",
    );
}

#[test]
fn signed_unsigned_cast_ok() {
    ok("main :: () => { const u: u32 = 7 as u32; const s: s64 = u as s64; const _ = s; };");
}

#[test]
fn given_enum_non_exhaustive() {
    let bag = typeck_err(include_str!(
        "../../../tests/cli/fixtures/given_enum_non_exhaustive.phx"
    ));
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::NonExhaustiveMatch { .. }))
    );
}

#[test]
fn unary_neg_not_and_comparisons_ok() {
    ok(include_str!(
        "../../../tests/cli/fixtures/compare_unary.phx"
    ));
}

fn typed(source: &str) -> phx_compiler::TypedProgram {
    compile_source(source, None)
        .unwrap_or_else(|e| panic!("expected ok: {e}"))
        .typed
}

#[test]
fn call_top_level_fn_ok() {
    ok("add :: (a: s32, b: s32) => s32 { a + b }; main :: () => { const x: s32 = add(1, 2); };");
}

#[test]
fn call_arity_mismatch() {
    let bag = typeck_err(
        "add :: (a: s32, b: s32) => s32 { a + b }; main :: () => { const _ = add(1); };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::ArityMismatch { .. }))
    );
}

#[test]
fn call_not_callable() {
    let bag = typeck_err("main :: () => { const n: s32 = 1; n(); };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::NotCallable { .. }))
    );
}

#[test]
fn main_layout_slot_count() {
    let typed = typed(
        "add :: (a: s32, b: s32) => s32 { a + b }; main :: () => { const base: s32 = 10; const step: s32 = 2; const sum: s32 = add(base, step); };",
    );
    let main_layout = typed
        .functions
        .iter()
        .find(|f| typed.entry == Some(f.def))
        .expect("main layout");
    assert_eq!(main_layout.local_count(), 3);
    let add_layout = typed
        .functions
        .iter()
        .find(|f| Some(f.def) != typed.entry)
        .expect("add layout");
    assert_eq!(add_layout.local_count(), 2);
}

#[test]
fn assign_ok() {
    ok("main :: () => { var x: s32 = 1; x = 2; };");
}

#[test]
fn assign_type_mismatch() {
    let bag = typeck_err("main :: () => { var x: s32 = 1; x = true; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::Mismatch { .. }))
    );
}

#[test]
fn assign_to_moved() {
    let bag = typeck_err(
        "Point :: struct { x: s32, y: s32, }; main :: () => { var p: Point = Point { x: 1, y: 2 }; var q: Point = p; p = Point { x: 0, y: 0 }; };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::MovedAssignTarget { .. }))
    );
}

#[test]
fn assign_moves_non_copyable() {
    let bag = typeck_err(
        "Point :: struct { x: s32, y: s32, }; main :: () => { var p: Point = Point { x: 1, y: 2 }; var q: Point = Point { x: 0, y: 0 }; var r: Point = p; const _ = p.x; };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::UseAfterMove { .. }))
    );
}

#[test]
fn enum_struct_variant_match_ok() {
    ok(include_str!(
        "../../../tests/cli/fixtures/enum_match_struct.phx"
    ));
}

#[test]
fn enum_struct_variant_lit_unknown_field() {
    let bag = typeck_err("R :: enum { Ok { v: s32 }, }; main :: () => { const _ = Ok { z: 1 }; };");
    assert!(bag.errors().iter().any(|e| {
        matches!(e, TypeCheckError::UnknownEnumVariantField { name, .. } if name == "z")
    }));
}

#[test]
fn enum_struct_variant_lit_missing_field() {
    let bag = typeck_err(
        "R :: enum { Ok { v: s32, w: s32 }, }; main :: () => { const _ = Ok { v: 1 }; };",
    );
    assert!(bag.errors().iter().any(|e| {
        matches!(e, TypeCheckError::MissingEnumVariantField { name, .. } if name == "w")
    }));
}

#[test]
fn struct_lit_unknown_field() {
    let bag = typeck_err(
        "Point :: struct { x: s32, y: s32, }; main :: () => { const _ = Point { x: 1, z: 2 }; };",
    );
    assert!(
        bag.errors().iter().any(|e| {
            matches!(e, TypeCheckError::UnknownStructField { name, .. } if name == "z")
        })
    );
}

#[test]
fn struct_lit_missing_field() {
    let bag = typeck_err(
        "Point :: struct { x: s32, y: s32, }; main :: () => { const _ = Point { x: 1 }; };",
    );
    assert!(
        bag.errors().iter().any(|e| {
            matches!(e, TypeCheckError::MissingStructField { name, .. } if name == "y")
        })
    );
}

#[test]
fn enum_match_non_exhaustive() {
    let bag = typeck_err(
        "Maybe :: enum { None, Some(s32), }; main :: () => { const m: Maybe = Some(1); const _ = match m { Some(x) => x; }; };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::NonExhaustiveMatch { .. }))
    );
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(e, TypeCheckError::NonExhaustiveMatch { .. }))
        .expect("non-exhaustive match");
    if let TypeCheckError::NonExhaustiveMatch { missing, .. } = err {
        assert!(missing.iter().any(|n| n == "None"));
    }
}

#[test]
fn enum_match_wildcard_exhaustive_ok() {
    ok(
        "Maybe :: enum { None, Some(s32), }; main :: () => { const m: Maybe = Some(1); const _ = match m { _ => 0; }; };",
    );
}

#[test]
fn index_array_ok() {
    ok("main :: () => { const a: [s32; 2] = [1, 2]; const x: s32 = a[0]; };");
}

#[test]
fn index_non_indexable_error() {
    let bag = typeck_err("main :: () => { const x: s32 = 1; const _ = x[0]; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::InvalidOperator { .. }))
    );
}

#[test]
fn enum_pattern_on_non_enum_scrutinee() {
    let bag = typeck_err(
        "Maybe :: enum { None, Some(s32), }; main :: () => { const _ = match 0 { None => 0; _ => 1; }; };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::Mismatch { .. }))
    );
}

#[test]
fn enum_tuple_pattern_on_non_enum_scrutinee() {
    let bag = typeck_err(
        "Maybe :: enum { None, Some(s32), }; main :: () => { const _ = match 0 { Some(x) => x; _ => 0; }; };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::Mismatch { .. }))
    );
}

#[test]
fn enum_match_user_enum_ok() {
    ok(include_str!("../../../tests/cli/fixtures/enum_match.phx"));
}

#[test]
fn type_alias_const_inference_ok() {
    ok("type Id = s32; main :: () => { const x: Id = 1; const _ = x; };");
}

#[test]
fn type_alias_assignability_ok() {
    ok("type Id = s32; main :: () => { const x: Id = 1; const y: s32 = x; const _ = y; };");
}

#[test]
fn type_alias_cast_ok() {
    ok("type Id = s32; main :: () => { const x: Id = 42 as Id; const _ = x; };");
}

#[test]
fn type_alias_meters_cast_ok() {
    ok("type Meters = s32; main :: () => { const x: Meters = 42 as Meters; const _ = x; };");
}

#[test]
fn type_alias_mismatch_still_errors() {
    let bag = typeck_err("type Id = s32; main :: () => { const x: Id = true; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::Mismatch { .. }))
    );
}

#[test]
fn match_unreachable_after_wildcard() {
    let bag =
        typeck_err("main :: () => { const x: s32 = match 0 { _ => 1; 2 => 2; }; const _ = x; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::UnreachableMatchArm { .. }))
    );
}

#[test]
fn match_unreachable_duplicate_literal() {
    let bag =
        typeck_err("main :: () => { const x: s32 = match 0 { 0 => 1; 0 => 2; }; const _ = x; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::UnreachableMatchArm { .. }))
    );
}

#[test]
fn match_unreachable_duplicate_enum_variant() {
    let bag = typeck_err(
        "Maybe :: enum { None, Some(s32) }; main :: () => { const m: Maybe = Some(1); const x: s32 = match m { Some(a) => a; Some(b) => b; }; const _ = x; };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::UnreachableMatchArm { .. }))
    );
}

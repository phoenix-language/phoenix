//! Integration tests for the type-checking pass.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use phx_compiler::CompileError;
use phx_compiler::{check_file, compile_source};
use phx_diagnostics::{TypeCheckBag, TypeCheckError};

fn ok(source: &str) {
    compile_source(source, None).unwrap_or_else(|e| panic!("expected ok: {e}"));
}

fn typeck_err(source: &str) -> TypeCheckBag {
    match compile_source(source, None) {
        Err(CompileError::TypeCheck { bag, .. }) => bag,
        Err(other) => panic!("expected type-check error, got {other}"),
        Ok(_) => panic!("expected type-check error"),
    }
}

fn has_unsupported(bag: &TypeCheckBag, needle: &str) -> bool {
    bag.errors().iter().any(|e| {
        matches!(&e.error, TypeCheckError::UnsupportedFeature { feature, .. } if feature.contains(needle))
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
    let source = "main :: () => { break; };";
    let bag = typeck_err(source);
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(&e.error, TypeCheckError::LoopControlOutsideLoop { .. }))
        .expect("LoopControlOutsideLoop");
    if let TypeCheckError::LoopControlOutsideLoop { keyword, span } = &err.error {
        assert_eq!(*keyword, "break");
        assert!(
            span.end > span.start,
            "expected non-zero break keyword span"
        );
        let keyword_start =
            u32::try_from(source.find("break").expect("break in source")).expect("offset fits u32");
        assert_eq!(span.start, keyword_start);
    }
}

#[test]
fn continue_outside_loop() {
    let source = "main :: () => { continue; };";
    let bag = typeck_err(source);
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(&e.error, TypeCheckError::LoopControlOutsideLoop { .. }))
        .expect("LoopControlOutsideLoop");
    if let TypeCheckError::LoopControlOutsideLoop { keyword, span } = &err.error {
        assert_eq!(*keyword, "continue");
        assert!(
            span.end > span.start,
            "expected non-zero continue keyword span"
        );
        let keyword_start = u32::try_from(source.find("continue").expect("continue in source"))
            .expect("offset fits u32");
        assert_eq!(span.start, keyword_start);
    }
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
            &e.error,
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
        .find(|e| matches!(&e.error, TypeCheckError::UseAfterMove { .. }))
        .expect("use-after-move");
    if let TypeCheckError::UseAfterMove { name, .. } = &err.error {
        assert_eq!(name, "p");
    }
    let interner = phx_syntax::Interner::new();
    let msg = phx_diagnostics::format_typecheck_error(source, &interner, &err.error);
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
            .any(|e| matches!(&e.error, TypeCheckError::Mismatch { .. }))
    );
}

#[test]
fn invalid_cast() {
    let bag = typeck_err("main :: () => { const x = 1 as bool; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::InvalidCast { .. }))
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
fn s32_as_f32_cast_ok() {
    ok("main :: () => { const n: s32 = 42; const f: f32 = n as f32; const _ = f; };");
}

#[test]
fn string_literal_and_str_as_u8_slice_ok() {
    ok(include_str!(
        "../../../tests/cli/fixtures/string_literal.phx"
    ));
}

#[test]
fn byte_string_as_str_literal_ok() {
    ok("main :: () => { const s: str = b\"hi\" as str; const _ = s; };");
}

#[test]
fn byte_string_as_str_const_fold_ok() {
    ok("main :: () => { const arr = b\"hi\"; const s: str = arr as str; const _ = s; };");
}

#[test]
fn invalid_utf8_byte_string_as_str() {
    let bag = typeck_err("main :: () => { const _ = b\"\\xFF\" as str; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::InvalidCast { .. }))
    );
}

#[test]
fn var_byte_array_as_str_rejected() {
    let bag = typeck_err("main :: () => { var arr: [u8; 2] = b\"hi\"; const _ = arr as str; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::InvalidCast { .. }))
    );
}

#[test]
fn given_enum_non_exhaustive() {
    let bag = typeck_err(include_str!(
        "../../../tests/cli/fixtures/given_enum_non_exhaustive.phx"
    ));
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::NonExhaustiveMatch { .. }))
    );
}

#[test]
fn given_enum_single_variant_ok() {
    ok(include_str!(
        "../../../tests/cli/fixtures/given_enum_single_variant.phx"
    ));
}

#[test]
fn factorial_recursion_ok() {
    ok(include_str!("../../../tests/cli/fixtures/factorial.phx"));
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
            .any(|e| matches!(&e.error, TypeCheckError::ArityMismatch { .. }))
    );
}

#[test]
fn call_not_callable() {
    let bag = typeck_err("main :: () => { const n: s32 = 1; n(); };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::NotCallable { .. }))
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
            .any(|e| matches!(&e.error, TypeCheckError::Mismatch { .. }))
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
            .any(|e| matches!(&e.error, TypeCheckError::MovedAssignTarget { .. }))
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
            .any(|e| matches!(&e.error, TypeCheckError::UseAfterMove { .. }))
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
        matches!(&e.error, TypeCheckError::UnknownEnumVariantField { name, .. } if name == "z")
    }));
}

#[test]
fn enum_struct_variant_lit_missing_field() {
    let bag = typeck_err(
        "R :: enum { Ok { v: s32, w: s32 }, }; main :: () => { const _ = Ok { v: 1 }; };",
    );
    assert!(bag.errors().iter().any(|e| {
        matches!(&e.error, TypeCheckError::MissingEnumVariantField { name, .. } if name == "w")
    }));
}

#[test]
fn struct_lit_unknown_field() {
    let bag = typeck_err(
        "Point :: struct { x: s32, y: s32, }; main :: () => { const _ = Point { x: 1, z: 2 }; };",
    );
    assert!(bag.errors().iter().any(|e| {
        matches!(&e.error, TypeCheckError::UnknownStructField { name, .. } if name == "z")
    }));
}

#[test]
fn struct_lit_generic_args_on_non_generic_errors() {
    let bag = typeck_err(
        "Point :: struct { x: s32, y: s32, }; main :: () => { const _ = Point::<s32> { x: 1, y: 2 }; };",
    );
    assert!(bag.errors().iter().any(|e| {
        matches!(
            &e.error,
            TypeCheckError::UnsupportedFeature {
                feature: "type arguments on non-generic struct literal",
                ..
            }
        )
    }));
}

#[test]
fn struct_lit_missing_field() {
    let bag = typeck_err(
        "Point :: struct { x: s32, y: s32, }; main :: () => { const _ = Point { x: 1 }; };",
    );
    assert!(bag.errors().iter().any(|e| {
        matches!(&e.error, TypeCheckError::MissingStructField { name, .. } if name == "y")
    }));
}

#[test]
fn enum_match_non_exhaustive() {
    let bag = typeck_err(
        "Maybe :: enum { None, Some(s32), }; main :: () => { const m: Maybe = Some(1); const _ = match m { Some(x) => x; }; };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::NonExhaustiveMatch { .. }))
    );
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(&e.error, TypeCheckError::NonExhaustiveMatch { .. }))
        .expect("non-exhaustive match");
    if let TypeCheckError::NonExhaustiveMatch { missing, .. } = &err.error {
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
            .any(|e| matches!(&e.error, TypeCheckError::InvalidOperator { .. }))
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
            .any(|e| matches!(&e.error, TypeCheckError::Mismatch { .. }))
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
            .any(|e| matches!(&e.error, TypeCheckError::Mismatch { .. }))
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
            .any(|e| matches!(&e.error, TypeCheckError::Mismatch { .. }))
    );
}

#[test]
fn match_unreachable_after_wildcard() {
    let bag =
        typeck_err("main :: () => { const x: s32 = match 0 { _ => 1; 2 => 2; }; const _ = x; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::UnreachableMatchArm { .. }))
    );
}

#[test]
fn match_unreachable_duplicate_literal() {
    let bag =
        typeck_err("main :: () => { const x: s32 = match 0 { 0 => 1; 0 => 2; }; const _ = x; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::UnreachableMatchArm { .. }))
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
            .any(|e| matches!(&e.error, TypeCheckError::UnreachableMatchArm { .. }))
    );
}

#[test]
fn deferred_typeck_for_in_loop() {
    let bag = typeck_err("main :: () => { for x in 0 { }; };");
    assert!(has_unsupported(&bag, "for-in"));
}

#[test]
fn deferred_typeck_range_expr() {
    let bag = typeck_err("main :: () => { const _ = 0..1; };");
    assert!(has_unsupported(&bag, "range"));
}

#[test]
fn deferred_typeck_lambda() {
    let bag = typeck_err("main :: () => { const _ = () => 1; };");
    assert!(has_unsupported(&bag, "lambda"));
}

#[test]
fn deferred_typeck_at_spawn() {
    let bag = typeck_err("f :: () => { }; main :: () => { @spawn(f); };");
    assert!(has_unsupported(&bag, "@spawn"));
}

#[test]
fn deferred_typeck_hash_derive() {
    let bag = typeck_err("#derive(Clone)\nmain :: () => { };");
    assert!(has_unsupported(&bag, "#derive"));
}

#[test]
fn shadowed_var_move_does_not_move_outer() {
    ok(
        "Point :: struct { x: s32, y: s32, }; main :: () => { var p: Point = Point { x: 1, y: 2 }; { var p: Point = Point { x: 3, y: 4 }; var q: Point = p; }; const _ = p.x; };",
    );
}

#[test]
fn bool_match_non_exhaustive_errors() {
    let bag = typeck_err("main :: () => { const _ = match false { true => 1; }; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::NonExhaustiveMatch { .. }))
    );
}

#[test]
fn int_match_requires_wildcard() {
    let bag = typeck_err("main :: () => { const _ = match 1 { 1 => 0; }; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::NonExhaustiveMatch { .. }))
    );
}

#[test]
fn recursive_type_alias_errors() {
    let bag = typeck_err("type A = B; type B = A; main :: () => { };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::RecursiveTypeAlias { .. }))
    );
}

#[test]
fn generic_fn_explicit_args_ok() {
    ok("id :: <t> (x: s32) => s32 { x }; main :: () => { const _: s32 = id :: <s32> (1); };");
}

#[test]
fn generic_fn_type_param_in_signature_ok() {
    ok("id :: <t> (x: t) => t { x }; main :: () => { const _: s32 = id :: <s32> (1); };");
}

#[test]
fn generic_fn_wrong_type_arg_count_errors() {
    let bag = typeck_err(
        "pair :: <a, b> (x: a, y: b) => a { x }; main :: () => { const _ = pair :: <s32> (1, 2); };",
    );
    assert!(bag.errors().iter().any(|e| {
        matches!(
            &e.error,
            TypeCheckError::ArityMismatch {
                expected: 2,
                found: 1,
                ..
            }
        )
    }));
}

#[test]
fn generic_fn_on_non_generic_errors() {
    let bag = typeck_err(
        "add :: (a: s32, b: s32) => s32 { a + b }; main :: () => { const _ = add :: <s32> (1, 2); };",
    );
    assert!(has_unsupported(&bag, "type arguments on non-generic call"));
}

#[test]
fn generic_struct_lit_ok() {
    ok(
        "Box :: <t> struct { v: t, }; main :: () => { const x = Box::<s32> { v: 1 }; const _: s32 = x.v; };",
    );
}

#[test]
fn generic_struct_lit_arity_mismatch_errors() {
    let bag = typeck_err(
        "Box :: <t> struct { v: t, }; main :: () => { const _ = Box::<s32, u32> { v: 1 }; };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| { matches!(&e.error, TypeCheckError::ArityMismatch { .. }) })
    );
}

#[test]
fn generic_enum_ctor_ok() {
    ok("Opt :: <t> enum { None, Some(t), }; main :: () => { const _ = Some :: <s32> (1); };");
}

#[test]
fn generic_type_alias_ok() {
    ok("type Pair<t> = (t, t); main :: () => { const p: Pair<s32> = (1, 2); };");
}

#[test]
fn generic_fn_end_to_end_compile() {
    ok("wrap :: <t> (x: t) => t { x }; main :: () => { const n: s32 = wrap :: <s32> (42); };");
}

#[test]
fn generic_call_ast_has_args() {
    let source = "id :: <t> (x: t) => t { x }; main :: () => { const n: s32 = id :: <s32> (42); };";
    let sf = phx_syntax::parse(source).expect("parse");
    let main = sf
        .program
        .items
        .iter()
        .find_map(|item| {
            if let phx_syntax::ast::decl::TopLevelDecl::Function(f) = &item.inner.decl
                && phx_syntax::Interner::resolve(&sf.interner, f.name.symbol) == "main"
            {
                return Some(f);
            }
            None
        })
        .expect("main");
    let init =
        main.body
            .inner
            .items
            .iter()
            .find_map(|item| {
                if let phx_syntax::ast::stmt::BlockItem::Stmt(
                    phx_syntax::ast::stmt::Stmt::Const { init, .. },
                ) = item
                {
                    Some(init)
                } else {
                    None
                }
            })
            .expect("const init");
    match &init.inner {
        phx_syntax::ast::expr::Expr::Postfix { ops, .. } => {
            let call = ops
                .iter()
                .find_map(|op| {
                    if let phx_syntax::ast::expr::PostfixOp::Call { args, .. } = op {
                        Some(args.len())
                    } else {
                        None
                    }
                })
                .expect("call op");
            assert_eq!(call, 1, "expected one call argument in AST");
        }
        other => panic!("expected Postfix call init, got {other:?}"),
    }
}

#[test]
fn generic_cli_fixtures_check_file_ok() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures");
    for name in ["generic_fn.phx", "generic_struct.phx", "generic_enum.phx"] {
        let path = root.join(name);
        let source = std::fs::read_to_string(&path).expect("read fixture");
        compile_source(&source, Some(&path))
            .unwrap_or_else(|e| panic!("compile_source {name}: {e}"));
        check_file(&path).unwrap_or_else(|e| panic!("check_file {name}: {e}"));
    }
}

#[test]
fn generic_fn_infer_from_args_ok() {
    ok("id :: <t> (x: t) => t { x }; main :: () => { const _: s32 = id(1); };");
}

#[test]
fn generic_fn_unconstrained_type_param_errors() {
    let bag = typeck_err("id :: <t> (x: s32) => s32 { x }; main :: () => { const _ = id(1); };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::InferenceFailed { .. }))
    );
}

#[test]
fn generic_fn_copyable_bound_ok() {
    ok(
        "max :: <t: Copyable> (a: t, b: t) => t { if a > b { a } else { b } }; main :: () => { const _: s32 = max(1, 2); };",
    );
}

#[test]
fn generic_fn_copyable_bound_fails_for_non_copyable_struct() {
    let bag = typeck_err(
        "Pair :: struct { a: s32, b: s32 }; max :: <t: Copyable> (a: t, b: t) => t { a }; main :: () => { const _ = max(Pair { a: 1, b: 2 }, Pair { a: 3, b: 4 }); };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::TraitNotSatisfied { .. }))
    );
}

#[test]
fn generic_fn_user_trait_bound_ok() {
    ok(
        "PartialEq :: trait { eq :: (self: Point, other: Point) => bool; }; Point :: struct { x: s32 }; Point :: impl :: PartialEq { eq :: (self: Point, other: Point) => bool { self.x == other.x }; }; same :: <t: PartialEq> (a: t, b: t) => bool { true }; main :: () => { const p = Point { x: 1 }; const q = Point { x: 2 }; const _: bool = same(p, q); };",
    );
}

#[test]
fn generic_impl_method_body_check_ok() {
    ok(
        "Box :: <t> struct { v: t }; Box :: <t> impl { get :: () => t { self.v }; }; main :: () => { };",
    );
}

#[test]
fn generic_impl_method_infer_ok() {
    ok(
        "Box :: <t> struct { v: t }; Box :: <t> impl { get :: () => t { self.v }; }; main :: () => { const b = Box :: <s32> { v: 10 }; const _: s32 = b.get(); };",
    );
}

#[test]
fn generic_impl_method_with_type_params_ok() {
    ok(
        "Box :: <t> struct { v: t }; Box :: <t> impl { id :: <u> (x: u) => u { x }; }; main :: () => { const b = Box :: <s32> { v: 1 }; const _: s32 = b.id(2); };",
    );
}

#[test]
fn trait_impl_missing_method_rejected() {
    let source = "PartialEq :: trait { eq :: (self: Point, other: Point) => bool; }; Point :: struct { x: s32 }; Point :: impl :: PartialEq { }; main :: () => { };";
    let bag = typeck_err(source);
    assert!(
        bag.errors().iter().any(|e| {
            matches!(
                &e.error,
                TypeCheckError::MissingTraitMethod {
                    method_name,
                    ..
                } if method_name == "eq"
            )
        }),
        "expected MissingTraitMethod for eq: {:?}",
        bag.errors()
    );
}

#[test]
fn parse_recovery_formats_multiple_carets() {
    let source = "main :: () => { const x = ; const y: s32 = ; };";
    let err = match compile_source(source, None) {
        Err(CompileError::Parse(bag)) => bag,
        other => panic!("expected parse error, got {other:?}"),
    };
    let formatted = CompileError::Parse(err).format_with_source(Some(source));
    assert!(
        formatted.contains("aborting due to 2 previous errors"),
        "expected multi-error footer:\n{formatted}"
    );
}

#[test]
fn return_ref_to_local_errors() {
    let source = "bad_ref :: () => &s32 { var x: s32 = 10; return &x; }; main :: () => { };";
    let bag = typeck_err(source);
    assert!(
        bag.errors()
            .iter()
            .any(|e| { matches!(&e.error, TypeCheckError::ReturnEscapesLocal { .. }) }),
        "expected ReturnEscapesLocal: {:?}",
        bag.errors()
    );
}

#[test]
fn return_slice_of_local_errors() {
    let source = "bad_slice :: () => [u8] { var arr: [u8; 4] = b\"WXYZ\"; return arr as [u8]; }; main :: () => { };";
    let bag = typeck_err(source);
    assert!(
        bag.errors()
            .iter()
            .any(|e| { matches!(&e.error, TypeCheckError::ReturnEscapesLocal { .. }) }),
        "expected ReturnEscapesLocal: {:?}",
        bag.errors()
    );
}

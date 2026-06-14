//! Integration tests for the type-checking pass.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_compiler::{
    CompileError, check_file, compile_source,
    unstable::{DefKind, TypedProgram},
};
use phx_diagnostics::{TypeCheckBag, TypeCheckError};
use phx_test::{compile_ok, expect_typeck_err};

fn typeck_err(source: &str) -> TypeCheckBag {
    expect_typeck_err(source)
}

fn typed_program(source: &str) -> TypedProgram {
    compile_source(source, None)
        .unwrap_or_else(|e| panic!("expected compile ok: {e}"))
        .typed
}

fn fn_def_names(typed: &TypedProgram) -> Vec<String> {
    typed
        .resolved
        .defs
        .iter()
        .filter(|d| d.kind == DefKind::Fn)
        .map(|d| typed.resolved.interner.resolve_display(d.name))
        .collect()
}

fn has_mangled_fn(typed: &TypedProgram, base: &str, type_suffix: &str) -> bool {
    let needle = format!("{base}${type_suffix}");
    fn_def_names(typed)
        .iter()
        .any(|name| name.eq_ignore_ascii_case(&needle))
}

fn fn_def_id(typed: &TypedProgram, name: &str) -> Option<phx_compiler::unstable::DefId> {
    typed.resolved.defs.iter().enumerate().find_map(|(i, d)| {
        if d.kind == DefKind::Fn && typed.resolved.interner.resolves_to(d.name, name) {
            Some(phx_compiler::unstable::DefId::from_raw(
                u32::try_from(i).ok()?,
            ))
        } else {
            None
        }
    })
}

fn is_generic_template(typed: &TypedProgram, def_name: &str) -> bool {
    let Some(template_id) = fn_def_id(typed, def_name) else {
        return false;
    };
    typed
        .specialized_from
        .values()
        .any(|&base| base == template_id)
}

fn has_unsupported(bag: &TypeCheckBag, needle: &str) -> bool {
    bag.errors().iter().any(|e| {
        matches!(&e.error, TypeCheckError::UnsupportedFeature { feature, .. } if feature.contains(needle))
    })
}

#[test]
fn empty_main_compile_ok() {
    compile_ok("main :: () => { };");
}

#[test]
fn while_loop_compile_ok() {
    compile_ok("main :: () => { var i: s32 = 0; while 3 > (i) { i = i + 1; }; };");
}

#[test]
fn loop_break_continue_compile_ok() {
    compile_ok("main :: () => { loop { break; }; loop { continue; }; };");
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
fn const_inference_compile_ok() {
    compile_ok("main :: () => { const x = 1; };");
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
fn question_mark_invalid_operand_without_result_context() {
    let bag = typeck_err("main :: () => { const _ = 1?; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| { matches!(&e.error, TypeCheckError::InvalidTryOperand { .. }) })
    );
}

#[test]
fn question_mark_ok_with_std_imports() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/std_try/src/main.phx");
    if !path.is_file() {
        return;
    }
    let unit = check_file(&path).unwrap_or_else(|e| panic!("std_try typeck: {e}"));
    assert!(
        !unit.typed.try_sites.is_empty(),
        "expected try_sites in std_try fixture"
    );
}

#[test]
fn try_result_from_conversion_ok() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/std_try_from/src/main.phx");
    if !path.is_file() {
        return;
    }
    check_file(&path).unwrap_or_else(|e| panic!("std_try_from typeck: {e}"));
}

#[test]
fn try_result_from_missing() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/std_try_from_missing/src/main.phx");
    if !path.is_file() {
        return;
    }
    let err = check_file(&path).expect_err("expected type error");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| { matches!(&e.error, TypeCheckError::TryErrorFromMissing { .. }) }),
        "expected TryErrorFromMissing: {:?}",
        bag.errors()
    );
}

#[test]
fn try_result_ok_mismatch() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/std_try_ok_mismatch/src/main.phx");
    if !path.is_file() {
        return;
    }
    let err = check_file(&path).expect_err("expected type error");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| { matches!(&e.error, TypeCheckError::InvalidTryOperand { .. }) }),
        "expected InvalidTryOperand for Ok type mismatch: {:?}",
        bag.errors()
    );
}

#[test]
fn try_result_identical_err_regression() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/std_try/src/main.phx");
    if !path.is_file() {
        return;
    }
    check_file(&path).unwrap_or_else(|e| panic!("std_try typeck: {e}"));
}

#[test]
fn use_after_move_error() {
    let source = "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; var q: Point = p; const _ = p.r; };";
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
fn str_assign_without_move() {
    compile_ok("main :: () => { const a: str = \"hi\"; const b = a; const _ = b; };");
}

#[test]
fn use_after_move_fn_arg() {
    let source = "Point :: struct { r: &s32 }; take :: (p: Point) => () { const _ = (); }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; take(p); const _ = p.r; };";
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
    assert!(msg.contains("note:"));
}

#[test]
fn if_move_in_then_use_in_else_ok() {
    compile_ok(
        "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; var c: bool = true; if c { var q: Point = p; } else { const _ = p.r; }; };",
    );
}

#[test]
fn if_else_if_move_isolation() {
    compile_ok(
        "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; var a: bool = true; var b: bool = false; if a { const _ = p.r; } else if b { var q: Point = p; } else { const _ = p.r; }; };",
    );
}

#[test]
fn match_move_in_first_arm_use_in_second_ok() {
    compile_ok(
        "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; var c: bool = true; match c { true => { var q: Point = p; }; false => { const _ = p.r; }; }; };",
    );
}

#[test]
fn if_move_then_use_after_if_errors() {
    let source = "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; var c: bool = true; if c { var q: Point = p; } else { const _ = (); }; const _ = p.r; };";
    let bag = typeck_err(source);
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(&e.error, TypeCheckError::UseAfterMove { .. }))
        .expect("use-after-move");
    if let TypeCheckError::UseAfterMove { name, .. } = &err.error {
        assert_eq!(name, "p");
    }
}

#[test]
fn match_move_then_use_after_match_errors() {
    let source = "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; var c: bool = true; match c { true => { var q: Point = p; }; false => { const _ = (); }; }; const _ = p.r; };";
    let bag = typeck_err(source);
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(&e.error, TypeCheckError::UseAfterMove { .. }))
        .expect("use-after-move");
    if let TypeCheckError::UseAfterMove { name, .. } = &err.error {
        assert_eq!(name, "p");
    }
}

#[test]
fn if_both_arms_use_binding_ok() {
    compile_ok(
        "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; var c: bool = true; if c { const _ = p.r; } else { const _ = p.r; }; const _ = p.r; };",
    );
}

#[test]
fn loop_use_then_move_in_body_errors() {
    let source = "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; loop { const _ = p.r; var q: Point = p; }; };";
    let bag = typeck_err(source);
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(&e.error, TypeCheckError::UseAfterMove { .. }))
        .expect("use-after-move");
    if let TypeCheckError::UseAfterMove { name, .. } = &err.error {
        assert_eq!(name, "p");
    }
}

#[test]
fn while_use_then_move_in_body_errors() {
    let source = "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; var c: bool = true; while c { const _ = p.r; var q: Point = p; }; };";
    let bag = typeck_err(source);
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(&e.error, TypeCheckError::UseAfterMove { .. }))
        .expect("use-after-move");
    if let TypeCheckError::UseAfterMove { name, .. } = &err.error {
        assert_eq!(name, "p");
    }
}

#[test]
fn loop_move_then_use_after_loop_errors() {
    let source = "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; loop { var q: Point = p; break; }; const _ = p.r; };";
    let bag = typeck_err(source);
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(&e.error, TypeCheckError::UseAfterMove { .. }))
        .expect("use-after-move");
    if let TypeCheckError::UseAfterMove { name, .. } = &err.error {
        assert_eq!(name, "p");
    }
}

#[test]
fn loop_move_then_break_ok() {
    compile_ok(
        "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; loop { var q: Point = p; break; }; };",
    );
}

#[test]
fn loop_both_s32_reassign_ok() {
    compile_ok("main :: () => { var i: s32 = 0; while i < 3 { i = i + 1; }; };");
}

#[test]
fn loop_inner_var_move_ok() {
    compile_ok(
        "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; loop { var p: Point = Point { r: &n }; var q: Point = p; }; };",
    );
}

#[test]
fn field_read_without_move_ok() {
    compile_ok(
        "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; const _ = p.r; };",
    );
}

#[test]
fn pattern_destructure_scrutinee_still_usable() {
    compile_ok(
        "Result :: enum { Ok { v: s32 }, Err { code: s32 }, }; main :: () => { var x: s32 = 1; var r: Result = Ok { v: x }; match r { Ok { v: y } => { const _ = r; const w = y; }; Err { code: c } => { const _ = c; }; }; };",
    );
}

#[test]
fn field_read_after_whole_move_errors() {
    let source = "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; var q: Point = p; const _ = p.r; };";
    let bag = typeck_err(source);
    let err = bag
        .errors()
        .iter()
        .find(|e| matches!(&e.error, TypeCheckError::UseAfterMove { .. }))
        .expect("use-after-move");
    if let TypeCheckError::UseAfterMove { name, .. } = &err.error {
        assert_eq!(name, "p");
    }
}

#[test]
fn field_assign_after_whole_move_errors() {
    let source = "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var m: s32 = 2; var p: Point = Point { r: &n }; var q: Point = p; p.r = &m; };";
    let bag = typeck_err(source);
    assert!(
        bag.errors().iter().any(|e| {
            matches!(
                &e.error,
                TypeCheckError::UseAfterMove { name, .. } if name == "p"
            ) || matches!(
                &e.error,
                TypeCheckError::MovedAssignTarget { name, .. } if name == "p"
            )
        }),
        "expected use-after-move or moved assign on `p`: {:?}",
        bag.errors()
    );
}

// MVP gap (PHX-025): field extraction does not invalidate the parent binding until post-MVP.
#[test]
fn partial_field_extract_not_tracked_mvp() {
    compile_ok(
        "Pair :: struct { a: Point, b: s32 }; Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Pair = Pair { a: Point { r: &n }, b: 1 }; var q: Point = p.a; const _ = p.a.r; };",
    );
}

#[test]
fn function_trailing_expr_return_compile_ok() {
    compile_ok("add :: (a: s32, b: s32) => s32 { a + b }; main :: () => { };");
}

#[test]
fn function_return_stmt_compile_ok() {
    compile_ok("f :: () => s32 { return 1; }; main :: () => { };");
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
fn cross_width_cast_compile_ok() {
    compile_ok(
        "main :: () => { const wide: s64 = 100 as s64; const narrow: u8 = 42 as u8; const bump: s64 = narrow as s64; const _ = wide + bump; };",
    );
}

#[test]
fn signed_unsigned_cast_compile_ok() {
    compile_ok("main :: () => { const u: u32 = 7 as u32; const s: s64 = u as s64; const _ = s; };");
}

#[test]
fn s32_as_f32_cast_compile_ok() {
    compile_ok("main :: () => { const n: s32 = 42; const f: f32 = n as f32; const _ = f; };");
}

#[test]
fn string_literal_and_str_as_u8_slice_compile_ok() {
    compile_ok(include_str!(
        "../../../tests/cli/fixtures/string_literal.phx"
    ));
}

#[test]
fn byte_string_as_str_literal_compile_ok() {
    compile_ok("main :: () => { const s: str = b\"hi\" as str; const _ = s; };");
}

#[test]
fn byte_string_as_str_const_fold_compile_ok() {
    compile_ok("main :: () => { const arr = b\"hi\"; const s: str = arr as str; const _ = s; };");
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
fn if_const_enum_non_exhaustive_compile_ok() {
    compile_ok(include_str!(
        "../../../tests/cli/fixtures/if_const_enum_non_exhaustive.phx"
    ));
}

#[test]
fn if_const_enum_single_variant_compile_ok() {
    compile_ok(include_str!(
        "../../../tests/cli/fixtures/if_const_enum_single_variant.phx"
    ));
}

#[test]
fn factorial_recursion_compile_ok() {
    compile_ok(include_str!("../../../tests/cli/fixtures/factorial.phx"));
}

#[test]
fn unary_neg_not_and_comparisons_compile_ok() {
    compile_ok(include_str!(
        "../../../tests/cli/fixtures/compare_unary.phx"
    ));
}

fn typed(source: &str) -> phx_compiler::unstable::TypedProgram {
    compile_source(source, None)
        .unwrap_or_else(|e| panic!("expected ok: {e}"))
        .typed
}

#[test]
fn call_top_level_fn_compile_ok() {
    compile_ok(
        "add :: (a: s32, b: s32) => s32 { a + b }; main :: () => { const x: s32 = add(1, 2); };",
    );
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
fn assign_compile_ok() {
    compile_ok("main :: () => { var x: s32 = 1; x = 2; };");
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
        "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; var q: Point = p; p = Point { r: &n }; };",
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
        "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; var q: Point = Point { r: &n }; var r: Point = p; const _ = p.r; };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::UseAfterMove { .. }))
    );
}

#[test]
fn enum_struct_variant_match_compile_ok() {
    compile_ok(include_str!(
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
fn enum_match_wildcard_exhaustive_compile_ok() {
    compile_ok(
        "Maybe :: enum { None, Some(s32), }; main :: () => { const m: Maybe = Some(1); const _ = match m { _ => 0; }; };",
    );
}

#[test]
fn index_array_compile_ok() {
    compile_ok("main :: () => { const a: [s32; 2] = [1, 2]; const x: s32 = a[0]; };");
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
fn enum_match_user_enum_compile_ok() {
    compile_ok(include_str!("../../../tests/cli/fixtures/enum_match.phx"));
}

#[test]
fn type_alias_const_inference_compile_ok() {
    compile_ok("type Id = s32; main :: () => { const x: Id = 1; const _ = x; };");
}

#[test]
fn type_alias_assignability_compile_ok() {
    compile_ok("type Id = s32; main :: () => { const x: Id = 1; const y: s32 = x; const _ = y; };");
}

#[test]
fn type_alias_cast_compile_ok() {
    compile_ok("type Id = s32; main :: () => { const x: Id = 42 as Id; const _ = x; };");
}

#[test]
fn type_alias_meters_cast_compile_ok() {
    compile_ok(
        "type Meters = s32; main :: () => { const x: Meters = 42 as Meters; const _ = x; };",
    );
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
fn typeck_for_in_loop_ok() {
    let source = r"
Option :: <t> enum { None, Some(t) };
IntoIter :: trait {
    type Item;
    type IntoIter;
    into_iter :: (self) => Self::IntoIter;
};
Iterator :: trait {
    type Item;
    next :: (self: &mut Self) => Option<Self::Item>;
};
R :: struct { n: s32 };
S :: struct {};
R :: impl :: IntoIter {
    type Item = s32;
    type IntoIter = S;
    into_iter :: (self) => S { S {} };
};
S :: impl :: Iterator {
    type Item = s32;
    next :: (self: &mut Self) => Option<s32> { None :: <s32> () };
};
main :: () => { for x in R { n: 0 } { const _ = x; }; };
";
    let typed = typed_program(source);
    let main_layout = typed
        .functions
        .iter()
        .find(|f| {
            typed
                .resolved
                .defs
                .get(f.def.index() as usize)
                .is_some_and(|d| typed.resolved.interner.resolves_to(d.name, "main"))
        })
        .expect("main layout");
    assert_eq!(
        main_layout.for_in_plans.len(),
        1,
        "expected one ForInPlan on main"
    );
}

#[test]
fn typeck_for_in_not_iterable() {
    let bag = typeck_err("main :: () => { for x in 0 { }; };");
    assert!(
        bag.errors().iter().any(|e| {
            matches!(
                &e.error,
                TypeCheckError::TraitNotSatisfied { trait_name, .. }
                    if trait_name == "IntoIter"
            )
        }),
        "expected IntoIter TraitNotSatisfied: {:?}",
        bag.errors()
    );
}

#[test]
fn deferred_typeck_range_expr() {
    let bag = typeck_err("main :: () => { const _ = 0..1; };");
    assert!(has_unsupported(&bag, "range"));
}

#[test]
fn deferred_typeck_range_pattern() {
    let bag = typeck_err("main :: () => { match 0 { 0..1 => (); _ => (); }; };");
    assert!(has_unsupported(&bag, "range pattern"));
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
fn deferred_typeck_at_send() {
    let bag = typeck_err("main :: () => { @send(1, 2); };");
    assert!(has_unsupported(&bag, "@send"));
}

#[test]
fn deferred_typeck_at_receive() {
    let bag = typeck_err("main :: () => { @receive(1); };");
    assert!(has_unsupported(&bag, "@receive"));
}

#[test]
fn deferred_typeck_at_reply() {
    let bag = typeck_err("main :: () => { @reply(1); };");
    assert!(has_unsupported(&bag, "@reply"));
}

#[test]
fn deferred_typeck_break_with_value() {
    let bag = typeck_err("main :: () => { loop { break 1; }; };");
    assert!(has_unsupported(&bag, "break"));
}

#[test]
fn deferred_typeck_hash_derive_on_fn() {
    let bag = typeck_err("#derive(Clone)\nmain :: () => { };");
    assert!(has_unsupported(&bag, "#derive"));
}

#[test]
fn typeck_derive_partialeq_ok() {
    compile_ok(
        "PartialEq :: trait { eq :: (self: &Self, other: &Self) => bool; }; \
         #derive(PartialEq) Point :: struct { x: s32, y: s32 }; \
         main :: () => { const p = Point { x: 1, y: 2 }; const q = Point { x: 1, y: 2 }; \
         const _: bool = p.eq(&q); };",
    );
}

#[test]
fn typeck_derive_unsupported_trait() {
    let err = compile_source(
        "#derive(Clone) Point :: struct { x: s32 }; main :: () => { };",
        None,
    )
    .expect_err("clone derive");
    let CompileError::Resolve { bag, .. } = err else {
        panic!("expected resolve error for unsupported derive");
    };
    assert!(
        bag.errors().iter().any(|e| {
            matches!(
                &e.error,
                phx_diagnostics::ResolveError::InvalidCfg { message, .. }
                    if message.contains("unsupported derive trait")
            )
        }),
        "expected unsupported derive: {:?}",
        bag.errors()
    );
}

#[test]
fn shadowed_var_move_does_not_move_outer() {
    compile_ok(
        "Point :: struct { r: &s32 }; main :: () => { var n: s32 = 1; var p: Point = Point { r: &n }; { var m: s32 = 2; var p: Point = Point { r: &m }; var q: Point = p; }; const _ = p.r; };",
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
fn generic_fn_explicit_args_compile_ok() {
    compile_ok(
        "id :: <t> (x: s32) => s32 { x }; main :: () => { const _: s32 = id :: <s32> (1); };",
    );
}

#[test]
fn generic_fn_type_param_in_signature_compile_ok() {
    compile_ok("id :: <t> (x: t) => t { x }; main :: () => { const _: s32 = id :: <s32> (1); };");
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
fn generic_struct_lit_compile_ok() {
    compile_ok(
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
fn generic_enum_ctor_compile_ok() {
    compile_ok(
        "Opt :: <t> enum { None, Some(t), }; main :: () => { const _ = Some :: <s32> (1); };",
    );
}

#[test]
fn generic_type_alias_compile_ok() {
    compile_ok("type Pair<t> = (t, t); main :: () => { const p: Pair<s32> = (1, 2); };");
}

#[test]
fn generic_fn_end_to_end_compile() {
    compile_ok(
        "wrap :: <t> (x: t) => t { x }; main :: () => { const n: s32 = wrap :: <s32> (42); };",
    );
}

#[test]
fn generic_call_ast_has_args() {
    let source = "id :: <t> (x: t) => t { x }; main :: () => { const n: s32 = id :: <s32> (42); };";
    let sf = phx_syntax::parse(source);
    assert!(!sf.has_errors(), "parse: {:?}", sf.errors);
    let sf = sf.value;
    let main = sf
        .program
        .items
        .iter()
        .find_map(|item| {
            if let phx_syntax::ast::decl::TopLevelDecl::Function(f) = &item.inner.decl
                && sf.interner.resolves_to(f.name.symbol, "main")
            {
                return Some(f);
            }
            None
        })
        .expect("main");
    let init = main
        .body
        .inner
        .items
        .iter()
        .find_map(|item| {
            if let phx_syntax::ast::stmt::BlockItem::Stmt(stmt) = item
                && let phx_syntax::ast::stmt::Stmt::Const { init, .. } = &stmt.inner
            {
                return Some(init);
            }
            None
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
    use phx_test::cli_fixture;
    for name in ["generic_fn.phx", "generic_struct.phx", "generic_enum.phx"] {
        let path = cli_fixture(name);
        let source = std::fs::read_to_string(&path).expect("read fixture");
        compile_source(&source, Some(&path))
            .unwrap_or_else(|e| panic!("compile_source {name}: {e}"));
        check_file(&path).unwrap_or_else(|e| panic!("check_file {name}: {e}"));
    }
}

#[test]
fn generic_fn_infer_from_args_compile_ok() {
    compile_ok("id :: <t> (x: t) => t { x }; main :: () => { const _: s32 = id(1); };");
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
fn generic_fn_copyable_bound_compile_ok() {
    compile_ok(
        "max :: <t: Copyable> (a: t, b: t) => t { if a > b { a } else { b } }; main :: () => { const _: s32 = max(1, 2); };",
    );
}

#[test]
fn generic_fn_copyable_bound_ok_for_all_copyable_struct() {
    compile_ok(
        "Pair :: struct { a: s32, b: s32 }; max :: <t: Copyable> (a: t, b: t) => t { a }; main :: () => { const _ = max(Pair { a: 1, b: 2 }, Pair { a: 3, b: 4 }); };",
    );
}

#[test]
fn generic_fn_clone_bound_fails_without_impl() {
    let bag = typeck_err(
        "Clone :: trait { clone :: (self: Pair) => Pair; }; Pair :: struct { a: s32, b: s32 }; dup :: <t: Clone> (x: t) => t { x.clone() }; main :: () => { const _ = dup(Pair { a: 1, b: 2 }); };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::TraitNotSatisfied { .. }))
    );
}

#[test]
fn unknown_trait_bound_errors() {
    let bag = typeck_err(
        "Pair :: struct { a: s32, b: s32 }; max :: <t: Pair> (a: t) => t { a }; main :: () => { const _ = max(1); };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::UnknownTraitBound { .. }))
    );
}

#[test]
fn float_does_not_satisfy_eq_trait_bound() {
    let bag = typeck_err(
        "Eq :: trait { }; max :: <t: Eq> (a: t) => t { a }; main :: () => { const _: f32 = max(1.0); };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::TraitNotSatisfied { .. }))
    );
}

#[test]
fn std_traits_fixture_typechecks() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/std_traits/src/main.phx");
    if !path.is_file() {
        return;
    }
    let unit = check_file(&path).unwrap_or_else(|e| panic!("std_traits typeck: {e}"));
    assert!(
        !unit.typed.primitive_method_sites.is_empty(),
        "expected primitive eq/clone method sites in std_traits"
    );
}

#[test]
fn std_prelude_fixture_typechecks_without_imports() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/std_prelude/src/main.phx");
    if !path.is_file() {
        return;
    }
    check_file(&path).unwrap_or_else(|e| panic!("std_prelude typeck: {e}"));
}

#[test]
fn generic_fn_user_trait_bound_compile_ok() {
    compile_ok(
        "PartialEq :: trait { eq :: (self: Point, other: Point) => bool; }; Point :: struct { x: s32 }; Point :: impl :: PartialEq { eq :: (self: Point, other: Point) => bool { self.x == other.x }; }; same :: <t: PartialEq> (a: t, b: t) => bool { true }; main :: () => { const p = Point { x: 1 }; const q = Point { x: 2 }; const _: bool = same(p, q); };",
    );
}

#[test]
fn generic_impl_method_body_check_compile_ok() {
    compile_ok(
        "Box :: <t> struct { v: t }; Box :: <t> impl { get :: () => t { self.v }; }; main :: () => { };",
    );
}

#[test]
fn generic_impl_method_infer_compile_ok() {
    compile_ok(
        "Box :: <t> struct { v: t }; Box :: <t> impl { get :: () => t { self.v }; }; main :: () => { const b = Box :: <s32> { v: 10 }; const _: s32 = b.get(); };",
    );
}

#[test]
fn generic_impl_method_with_type_params_compile_ok() {
    compile_ok(
        "Box :: <t> struct { v: t }; Box :: <t> impl { id :: <u> (x: u) => u { x }; }; main :: () => { const b = Box :: <s32> { v: 1 }; const _: s32 = b.id(2); };",
    );
}

#[test]
fn trait_default_empty_impl_typechecks() {
    compile_ok(
        "Counter :: struct { n: s32 }; Zero :: trait { zero :: () => Self { Counter { n: 0 } }; }; Counter :: impl :: Zero { }; main :: () => { const c: Counter = Counter::zero(); const _ = c.n; };",
    );
}

#[test]
fn trait_default_with_default_body_exhaustive() {
    compile_ok(
        "Greet :: trait { msg :: () => [u8; 4] { [72 as u8, 73 as u8, 0 as u8, 0 as u8]; }; }; Point :: struct { x: s32 }; Point :: impl :: Greet { }; main :: () => { const p = Point { x: 1 }; const _ = p.msg(); };",
    );
}

#[test]
fn trait_default_override_wins() {
    compile_ok(
        "Zero :: trait { zero :: () => Self { 0 }; }; Counter :: struct { n: s32 }; Counter :: impl :: Zero { zero :: () => Counter { Counter { n: 99 } }; }; main :: () => { const c: Counter = Counter::zero(); const _ = c.n; };",
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
fn parse_recovery_merges_type_errors_in_output() {
    let source = "main :: () => { const x = ; var bad: s32 = b\"not\"; };";
    let err = match compile_source(source, None) {
        Err(
            e @ CompileError::TypeCheck {
                prior_parse: Some(_),
                ..
            },
        ) => e,
        other => panic!("expected type error with prior parse, got {other:?}"),
    };
    let formatted = err.format_with_source(Some(source));
    assert!(
        formatted.matches('^').count() >= 2,
        "expected parse carets in merged output:\n{formatted}"
    );
    assert!(
        formatted.contains("type mismatch") || formatted.contains("expected"),
        "expected type diagnostic in merged output:\n{formatted}"
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

#[test]
fn generic_trait_signature_compile_ok() {
    compile_ok(
        "Container :: <t> trait { get :: () => t; }; Pair :: <a, b> struct { a: a, b: b, }; main :: () => { };",
    );
}

#[test]
fn generic_fn_mangles_def_name() {
    let source = "id :: <t> (x: t) => t { x }; main :: () => { const _: s32 = id :: <s32> (1); };";
    let typed = typed_program(source);
    assert!(
        has_mangled_fn(&typed, "id", "s32"),
        "expected mangled fn def id$s32, got {:?}",
        fn_def_names(&typed)
    );
    assert!(
        is_generic_template(&typed, "id"),
        "expected id template to be replaced by monomorphization"
    );
    let template_id = fn_def_id(&typed, "id").expect("id template def");
    assert!(
        !typed.functions.iter().any(|f| f.def == template_id),
        "generic template should not appear in lowered function layouts"
    );
}

#[test]
fn generic_enum_specialized_layout() {
    let source =
        "Opt :: <t> enum { None, Some(t), }; main :: () => { const _ = Some :: <s32> (1); };";
    let typed = typed_program(source);
    assert_eq!(
        typed.layout.specialized_enums.len(),
        1,
        "expected one monomorphized enum layout"
    );
}

#[test]
fn generic_enum_ctor_infer_compile_ok() {
    compile_ok("Opt :: <t> enum { None, Some(t), }; main :: () => { const _ = Some(1); };");
    let typed =
        typed_program("Opt :: <t> enum { None, Some(t), }; main :: () => { const _ = Some(1); };");
    assert_eq!(typed.layout.specialized_enums.len(), 1);
}

#[test]
fn generic_dual_fn_instantiation_mangles_two_defs() {
    let source = "id :: <t> (x: t) => t { x }; main :: () => { const a: s32 = id :: <s32> (1); const b: bool = id :: <bool> (true); const _ = a; };";
    let typed = typed_program(source);
    assert!(
        has_mangled_fn(&typed, "id", "s32"),
        "expected id$s32 among {:?}",
        fn_def_names(&typed)
    );
    assert!(
        has_mangled_fn(&typed, "id", "bool"),
        "expected id$bool among {:?}",
        fn_def_names(&typed)
    );
    assert_eq!(
        typed.specialized_from.len(),
        2,
        "expected two specialized function defs"
    );
}

#[test]
fn generic_dual_enum_instantiation_two_layouts() {
    let source = "Opt :: <t> enum { None, Some(t), }; main :: () => { const a = Some :: <s32> (1); const b = Some :: <bool> (true); const _ = a; };";
    let typed = typed_program(source);
    assert_eq!(
        typed.layout.specialized_enums.len(),
        2,
        "expected two monomorphized enum layouts"
    );
}

#[test]
fn generic_copyable_bound_fails_on_second_instantiation_site() {
    let source = "Holder :: struct { r: &s32 }; max :: <t: Copyable> (a: t, b: t) => t { a }; main :: () => { const ok: s32 = max(1, 2); var n: s32 = 1; const bad = max(Holder { r: &n }, Holder { r: &n }); const _ = bad; };";
    let bag = typeck_err(source);
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::TraitNotSatisfied { .. })),
        "expected TraitNotSatisfied on non-Copyable Pair instantiation: {:?}",
        bag.errors()
    );
}

#[test]
fn generic_enum_match_s32_compile_ok() {
    compile_ok(
        "Opt :: <t> enum { None, Some(t), }; main :: () => { const x = Some :: <s32> (1); const n: s32 = match x { None => 0; Some(v) => v; }; const _ = n; };",
    );
}

#[test]
fn generic_enum_match_dual_instantiation_compile_ok() {
    compile_ok(
        "Opt :: <t> enum { None, Some(t), }; main :: () => { const a = Some :: <s32> (1); const b = Some :: <bool> (true); const n: s32 = match a { None => 0; Some(v) => v; }; const m: bool = match b { None => false; Some(w) => w; }; const _ = n; const _discard = m; };",
    );
}

#[test]
fn generic_two_param_enum_match_compile_ok() {
    compile_ok(
        "Pair :: <a, b> enum { Ok(a), Err(b), }; main :: () => { const r = Ok :: <s32, bool> (1); const n: s32 = match r { Ok(v) => v; Err(_) => 0; }; const _ = n; };",
    );
}

#[test]
fn result_match_struct_payloads_typecheck() {
    compile_ok(
        "Cfg :: struct { n: s32 }; AppE :: struct { c: s32 }; Pair :: <a, b> enum { Ok(a), Err(b), }; main :: () => { const r = Ok :: <Cfg, AppE> (Cfg { n: 42 }); const v: s32 = match r { Ok(c) => c.n; Err(e) => e.c; }; const _ = v; };",
    );
}

#[test]
fn result_match_non_exhaustive_rejected() {
    let bag = typeck_err(
        "Pair :: <a, b> enum { Ok(a), Err(b), }; main :: () => { const r = Ok :: <s32, s32> (1); const _ = match r { Ok(v) => v; }; };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::NonExhaustiveMatch { .. })),
        "expected NonExhaustiveMatch: {:?}",
        bag.errors()
    );
}

#[test]
fn result_if_const_ok_binds_struct() {
    compile_ok(
        "Cfg :: struct { n: s32 }; AppE :: struct { c: s32 }; Pair :: <a, b> enum { Ok(a), Err(b), }; main :: () => { const r = Ok :: <Cfg, AppE> (Cfg { n: 7 }); if const Ok(c) = r { const v: s32 = c.n; const _ = v; }; };",
    );
}

#[test]
fn match_scrutinee_registers_generic_enum_mono() {
    let source = "Cfg :: struct { n: s32 }; AppE :: struct { c: s32 }; Pair :: <a, b> enum { Ok(a), Err(b), }; handle :: (r: Pair<Cfg, AppE>) => s32 { match r { Ok(c) => c.n; Err(e) => e.c; } }; main :: () => { };";
    let typed = typed_program(source);
    assert!(
        !typed.layout.specialized_enums.is_empty(),
        "expected specialized_enums from match scrutinee on generic enum parameter"
    );
}

#[test]
fn trait_assoc_type_impl_compile_ok() {
    compile_ok(
        "Iterator :: trait { type Item; peek :: () => Self::Item; }; Counter :: struct { n: s32 }; Counter :: impl :: Iterator { type Item = s32; peek :: () => s32 { self.n }; }; main :: () => { const c = Counter { n: 42 }; const _: s32 = c.peek(); };",
    );
}

#[test]
fn trait_impl_missing_associated_type_rejected() {
    let source = "Iterator :: trait { type Item; peek :: () => Self::Item; }; Counter :: struct { n: s32 }; Counter :: impl :: Iterator { peek :: () => s32 { self.n }; }; main :: () => { };";
    let bag = typeck_err(source);
    assert!(
        bag.errors().iter().any(|e| {
            matches!(
                &e.error,
                TypeCheckError::MissingAssociatedType {
                    assoc_name,
                    ..
                } if assoc_name == "Item"
            )
        }),
        "expected MissingAssociatedType for Item: {:?}",
        bag.errors()
    );
}

#[test]
fn generic_user_trait_bound_at_mono_site_compile_ok() {
    compile_ok(
        "Marker :: trait { }; Tagged :: struct { n: s32 }; Tagged :: impl :: Marker { }; identity :: <t: Marker> (x: t) => t { x }; main :: () => { const t = Tagged { n: 1 }; const _: Tagged = identity(t); };",
    );
}

#[test]
fn type_alias_forwards_trait_bound_at_mono_site() {
    compile_ok(
        "Marker :: trait { }; Tagged :: struct { n: s32 }; Tagged :: impl :: Marker { }; type Alias = Tagged; identity :: <t: Marker> (x: t) => t { x }; main :: () => { const t = Alias { n: 1 }; const _: Alias = identity(t); };",
    );
}

#[test]
fn associated_from_call_compile_ok() {
    compile_ok(
        "FromLocal :: <source> trait { from :: (value: source) => Self; }; Wrap :: struct { n: s32 }; Wrap :: impl :: FromLocal<s32> { from :: (value: s32) => Wrap { Wrap { n: value } }; }; main :: () => { const w: Wrap = Wrap::from(42); const _ = w.n; };",
    );
}

#[test]
fn generic_default_fills_trailing_type_arg() {
    compile_ok(
        "Pair :: <t, u = s32> struct { a: t, b: u }; main :: () => { var x: Pair<bool> = Pair :: <bool> { a: true, b: 1 }; const _ = x; };",
    );
}

#[test]
fn associated_fn_type_generics_path_compile_ok() {
    compile_ok(
        "Box :: <t> struct { n: t }; Box :: <t> impl { new :: (n: t) => Box<t> { Box :: <t> { n: n } }; }; main :: () => { const b = Box :: <s32> :: new(1); const _ = b.n; };",
    );
}

#[test]
fn generic_from_bound_at_mono_site_compile_ok() {
    compile_ok(
        "FromLocal :: <source> trait { from :: (value: source) => Self; }; Wrap :: struct { n: s32 }; Wrap :: impl :: FromLocal<s32> { from :: (value: s32) => Wrap { Wrap { n: value } }; }; convert :: <t: FromLocal<s32>> (x: s32) => t { t::from(x) }; main :: () => { const w: Wrap = convert :: <Wrap>(42); const _ = w.n; };",
    );
}

#[test]
fn from_bound_missing_impl_errors() {
    let source = "FromLocal :: <source> trait { from :: (value: source) => Self; }; Pair :: struct { a: s32 }; to :: <u, t: FromLocal<u>> (x: u) => t { t::from(x) }; main :: () => { const _: Pair = to :: <s32, Pair>(1); };";
    let bag = typeck_err(source);
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::TraitNotSatisfied { .. })),
        "expected TraitNotSatisfied for missing From impl: {:?}",
        bag.errors()
    );
}

#[test]
fn try_from_assoc_type_impl_compile_ok() {
    compile_ok(
        "Result :: <ok, err> enum { Ok(ok), Err(err), }; TryFromLocal :: <source> trait { type Error; try_from :: (value: source) => Result<Self, Self::Error>; }; Box :: struct { n: s32 }; Box :: impl :: TryFromLocal<s32> { type Error = s32; try_from :: (value: s32) => Result<Box, s32> { if value >= 0 { Ok(Box { n: value }) } else { Err(value) } }; }; main :: () => { const b = Box::try_from(3); const _ = b; };",
    );
}

#[test]
fn fn_pointer_indirect_call_ok() {
    let source = "double :: (x: s32) => s32 { x + x }; apply :: (f: :: (s32) => s32, x: s32) => s32 { f(x) }; main :: () => { const n: s32 = apply(double, 3); const _ = n; };";
    let typed = typed_program(source);
    assert!(
        !typed.indirect_call_sites.is_empty(),
        "expected indirect call site for apply(double, 3)"
    );
}

#[test]
fn fn_pointer_value_is_copyable() {
    compile_ok(
        "double :: (x: s32) => s32 { x + x }; main :: () => { const f = double; const g = f; const _ = g(1); };",
    );
}

#[test]
fn extern_call_requires_unsafe() {
    let source = "extern \"C\" stub :: (x: s32) => s32; main :: () => { stub(1); };";
    let bag = typeck_err(source);
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::ExternCallRequiresUnsafe { .. })),
        "expected ExternRequiresUnsafe: {:?}",
        bag.errors()
    );
}

#[test]
fn dealloc_bytes_requires_unsafe() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/heap_dealloc_unsafe/src/main.phx");
    if !path.is_file() {
        return;
    }
    let err = check_file(&path).expect_err("expected dealloc_bytes outside unsafe to fail");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::IntrinsicRequiresUnsafe { .. })),
        "expected IntrinsicRequiresUnsafe: {:?}",
        bag.errors()
    );
}

#[test]
fn heap_dealloc_fixture_typechecks() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/heap_dealloc/src/main.phx");
    if !path.is_file() {
        return;
    }
    let unit = check_file(&path).expect("heap_dealloc typecheck");
    assert!(
        unit.typed.intrinsic_call_sites.len() >= 2,
        "expected at least alloc_bytes + dealloc_bytes intrinsic sites, got {}",
        unit.typed.intrinsic_call_sites.len()
    );
}

#[test]
fn alloc_bytes_requires_unsafe() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/heap_alloc_unsafe/src/main.phx");
    if !path.is_file() {
        return;
    }
    let err = check_file(&path).expect_err("expected alloc_bytes outside unsafe to fail");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::IntrinsicRequiresUnsafe { .. })),
        "expected IntrinsicRequiresUnsafe: {:?}",
        bag.errors()
    );
}

#[test]
fn slice_from_raw_parts_requires_unsafe() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/heap_slice_unsafe/src/main.phx");
    if !path.is_file() {
        return;
    }
    let err = check_file(&path).expect_err("expected slice_from_raw_parts outside unsafe to fail");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::IntrinsicRequiresUnsafe { .. })),
        "expected IntrinsicRequiresUnsafe: {:?}",
        bag.errors()
    );
}

#[test]
fn heap_slice_fixture_typechecks() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/heap_slice/src/main.phx");
    if !path.is_file() {
        return;
    }
    let unit = check_file(&path).expect("heap_slice typecheck");
    assert!(
        unit.typed.intrinsic_call_sites.len() >= 2,
        "expected at least alloc_bytes + slice_from_raw_parts intrinsic sites, got {}",
        unit.typed.intrinsic_call_sites.len()
    );
}

#[test]
fn heap_alloc_fixture_typechecks() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/heap_alloc/src/main.phx");
    if !path.is_file() {
        return;
    }
    let unit = check_file(&path).expect("heap_alloc typecheck");
    assert!(
        !unit.typed.intrinsic_call_sites.is_empty(),
        "expected intrinsic_call_sites for alloc_bytes"
    );
    // Reuses `buf` after `*buf = …` — would fail use-after-move if *mut u8 were non-Copyable.
}

#[test]
fn extern_call_in_unsafe_ok() {
    compile_ok("extern \"C\" stub :: (x: s32) => s32; main :: () => { unsafe { stub(1); }; };");
}

#[test]
fn unsafe_fn_call_requires_unsafe() {
    let source = "unsafe leak :: () => *mut u8 { 0 as *mut u8 }; main :: () => { leak(); };";
    let bag = typeck_err(source);
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::UnsafeFnCallRequiresUnsafe { .. })),
        "expected UnsafeFnCallRequiresUnsafe: {:?}",
        bag.errors()
    );
}

#[test]
fn unsafe_trait_requires_unsafe_impl() {
    let source = "A :: unsafe trait { f :: () => (); }; T :: struct {}; T :: impl :: A { f :: () => () {}; }; main :: () => { };";
    let bag = typeck_err(source);
    assert!(
        bag.errors().iter().any(|e| matches!(
            &e.error,
            TypeCheckError::UnsafeTraitRequiresUnsafeImpl { .. }
        )),
        "expected UnsafeTraitRequiresUnsafeImpl: {:?}",
        bag.errors()
    );
}

#[test]
fn unsafe_impl_of_safe_trait_rejected() {
    let source = "A :: trait { f :: () => (); }; T :: struct {}; T :: unsafe impl :: A { f :: () => () {}; }; main :: () => { };";
    let bag = typeck_err(source);
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::UnsafeImplOfSafeTrait { .. })),
        "expected UnsafeImplOfSafeTrait: {:?}",
        bag.errors()
    );
}

#[test]
fn unsafe_trait_method_call_requires_unsafe() {
    let source = "A :: unsafe trait { f :: (self: &mut Self) => (); }; T :: struct {}; T :: unsafe impl :: A { f :: (self: &mut Self) => () {}; }; main :: () => { var t: T = T {}; t.f(); };";
    let bag = typeck_err(source);
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::UnsafeFnCallRequiresUnsafe { .. })),
        "expected UnsafeFnCallRequiresUnsafe: {:?}",
        bag.errors()
    );
}

#[test]
fn allocator_smoke_unsafe_fail_fixture() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/allocator_smoke_unsafe_fail/src/main.phx");
    if !path.is_file() {
        return;
    }
    let err = check_file(&path).expect_err("expected alloc outside unsafe to fail");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::UnsafeFnCallRequiresUnsafe { .. })),
        "expected UnsafeFnCallRequiresUnsafe: {:?}",
        bag.errors()
    );
}

#[test]
fn drop_scope_exit_plans_drop_event() {
    let source = r"
Drop :: trait { drop :: (self) => (); };
Wrapper :: struct {};
Wrapper :: impl :: Drop { drop :: (self) => () {}; };
main :: () => { { const w = Wrapper {}; } };
";
    let unit = compile_source(source, None).expect("compile drop program");
    let main_layout = unit
        .typed
        .functions
        .iter()
        .find(|f| Some(f.def) == unit.typed.entry)
        .expect("main layout");
    assert_eq!(
        main_layout.drop_events.len(),
        1,
        "expected one scope-exit drop for `w`"
    );
}

#[test]
fn drop_manual_call_use_after_move() {
    let source = r"
Drop :: trait { drop :: (self) => (); };
Wrapper :: struct {};
Wrapper :: impl :: Drop { drop :: (self) => () {}; };
main :: () => { const w = Wrapper {}; w.drop(); const _ = w; };
";
    let bag = typeck_err(source);
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::UseAfterMove { .. })),
        "expected use-after-move after manual drop: {:?}",
        bag.errors()
    );
}

#[test]
fn tuple_struct_ctor_and_field_ok() {
    compile_ok(
        "Millimeters :: struct(s32); \
         main :: () => { const m: Millimeters = Millimeters(500); const x: s32 = m.0; const _ = x; };",
    );
}

#[test]
fn tuple_struct_implicit_inner_assign_errors() {
    let bag = typeck_err(
        "Millimeters :: struct(s32); main :: () => { const m: Millimeters = Millimeters(1); const x: s32 = m; };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::Mismatch { .. })),
        "expected mismatch: {:?}",
        bag.errors()
    );
}

#[test]
fn tuple_struct_fn_param_mismatch_errors() {
    let bag = typeck_err(
        "Millimeters :: struct(s32); \
         f :: (x: s32) => () { const _ = (); }; \
         main :: () => { const m: Millimeters = Millimeters(1); f(m); };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::Mismatch { .. })),
        "expected mismatch: {:?}",
        bag.errors()
    );
}

#[test]
fn tuple_struct_single_field_cast_ok() {
    compile_ok(
        "Millimeters :: struct(s32); \
         main :: () => { const m: Millimeters = 7 as Millimeters; const x: s32 = m as s32; const _ = x; };",
    );
}

#[test]
fn copyable_drop_conflict_rejected() {
    let source = r"
Copyable :: trait {};
Drop :: trait { drop :: (self) => (); };
Wrapper :: struct {};
Wrapper :: impl :: Copyable {};
Wrapper :: impl :: Drop { drop :: (self) => () {}; };
main :: () => {};
";
    let bag = typeck_err(source);
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::CopyableDropConflict { .. })),
        "expected CopyableDropConflict: {:?}",
        bag.errors()
    );
}

#[test]
fn missing_fn_def_emits_internal_error_instead_of_def_zero() {
    use phx_compiler::unstable::{resolve, type_check};

    let source = "main :: () => { };";
    let file = phx_syntax::parse(source);
    assert!(!file.has_errors(), "parse: {:?}", file.errors_bag());
    let mut resolved = resolve(&file.value).expect("resolve");
    resolved.defs.retain(|d| d.kind != DefKind::Fn);
    let bag = type_check(resolved).expect_err("expected typeck failure");
    assert!(
        bag.errors().iter().any(|e| {
            matches!(
                &e.error,
                TypeCheckError::InternalError { detail, .. }
                    if detail.contains("unresolved function definition")
            )
        }),
        "expected InternalError for missing fn def: {:?}",
        bag.errors()
    );
}

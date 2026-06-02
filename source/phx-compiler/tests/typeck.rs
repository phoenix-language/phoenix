//! Integration tests for the type-checking pass.

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

#[test]
fn empty_main_ok() {
    ok("main :: () => { };");
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
fn invalid_question_mark_outside_result_fn() {
    let bag = typeck_err("main :: () => { const _ = 1?; };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::InvalidQuestionMark { .. }))
    );
}

#[test]
fn use_after_move_error() {
    let bag = typeck_err(
        "Point :: struct { x: s32, y: s32, }; main :: () => { var p: Point = Point { x: 1, y: 2 }; var q: Point = p; const _ = p.x; };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::UseAfterMove { .. }))
    );
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
fn discard_ok_expr_stmt() {
    let bag = typeck_err("main :: () => { Ok(1); };");
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::DiscardResultOrOption { .. }))
    );
}

#[test]
fn discard_result_call_in_result_fn() {
    let bag = typeck_err(
        "f :: () => Result<s32, s32> { Ok(1) }; g :: () => Result<s32, s32> { f(); }; main :: () => { };",
    );
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(e, TypeCheckError::DiscardResultOrOption { .. }))
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
fn ok_expr() {
    ok("main :: () => { const x: Result<s32, s32> = Ok(1); };");
}

#[test]
fn some_expr() {
    ok("main :: () => { const x: Option<s32> = Some(1); };");
}

#[test]
fn err_expr() {
    ok("main :: () => { const x: Result<s32, s32> = Err(1); };");
}

#[test]
fn none_with_annotation() {
    ok("main :: () => { const x: Option<s32> = None; };");
}

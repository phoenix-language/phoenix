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

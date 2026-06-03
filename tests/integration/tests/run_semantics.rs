//! End-to-end semantic tests: verify computed values, not just clean execution.

use std::path::Path;

use phx_bytecode::ScalarValue;
use phx_bytecode::verify;
use phx_compiler::compile_to_module;
use phx_vm::{Value, run_captured};

fn compile_fixture(name: &str) -> phx_bytecode::BytecodeModule {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../cli/fixtures")
        .join(name);
    let module = compile_to_module(&path).unwrap_or_else(|e| panic!("compile {name}: {e}"));
    verify(&module).unwrap_or_else(|e| panic!("verify {name}: {e}"));
    module
}

fn scalar_i32(value: Value) -> Option<i32> {
    match value {
        Value::Scalar(ScalarValue::I32(v)) => Some(v),
        _ => None,
    }
}

fn scalar_bool(value: Value) -> Option<bool> {
    match value {
        Value::Scalar(ScalarValue::Bool(v)) => Some(v),
        _ => None,
    }
}

fn main_locals_contain_i32(module: &phx_bytecode::BytecodeModule, expected: i32) {
    let capture = run_captured(module).unwrap_or_else(|e| panic!("run: {e}"));
    assert!(
        capture
            .main_locals
            .iter()
            .any(|v| scalar_i32(*v) == Some(expected)),
        "expected s32 {expected} in main locals: {:?}",
        capture.main_locals
    );
}

fn main_locals_contain_bool(module: &phx_bytecode::BytecodeModule, expected: bool) {
    let capture = run_captured(module).unwrap_or_else(|e| panic!("run: {e}"));
    assert!(
        capture
            .main_locals
            .iter()
            .any(|v| scalar_bool(*v) == Some(expected)),
        "expected bool {expected} in main locals: {:?}",
        capture.main_locals
    );
}

#[test]
fn sample_arithmetic_computes_sum() {
    let module = compile_fixture("sample.phx");
    main_locals_contain_i32(&module, 12);
    main_locals_contain_bool(&module, true);
}

#[test]
fn enum_match_extracts_payload() {
    let module = compile_fixture("enum_match.phx");
    main_locals_contain_i32(&module, 42);
}

#[test]
fn struct_method_sums_fields() {
    let module = compile_fixture("struct_method.phx");
    main_locals_contain_i32(&module, 7);
}

#[test]
fn struct_point_sums_via_function() {
    let module = compile_fixture("struct_point.phx");
    main_locals_contain_i32(&module, 7);
}

#[test]
fn trait_eq_method_returns_true() {
    let module = compile_fixture("trait_eq.phx");
    main_locals_contain_bool(&module, true);
}

//! End-to-end semantic tests: verify computed values, not just clean execution.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use phx_bytecode::ScalarValue;
use phx_bytecode::verify;
use phx_compiler::{
    build_project, compile_to_module, compile_to_module_with_module_path, discover_project,
    load_project_binary,
};
use phx_vm::{Value, run_captured};

fn compile_fixture(name: &str) -> phx_bytecode::BytecodeModule {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../cli/fixtures")
        .join(name);
    let module = compile_to_module(&path).unwrap_or_else(|e| panic!("compile {name}: {e}"));
    verify(&module).unwrap_or_else(|e| panic!("verify {name}: {e}"));
    module
}

fn compile_fixture_with_module_root(
    entry: &str,
    module_root: &Path,
) -> phx_bytecode::BytecodeModule {
    let path = module_root.join(entry);
    let label = format!("{}/{}", module_root.display(), entry);
    let module = compile_to_module_with_module_path(&path, module_root)
        .unwrap_or_else(|e| panic!("compile {label}: {e}"));
    verify(&module).unwrap_or_else(|e| panic!("verify {label}: {e}"));
    module
}

fn compile_mvp_acceptance() -> phx_bytecode::BytecodeModule {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/fixtures/mvp_acceptance");
    let config = discover_project(&root).unwrap_or_else(|e| panic!("discover mvp_acceptance: {e}"));
    build_project(&config, None, true).unwrap_or_else(|e| panic!("build mvp_acceptance: {e}"));
    let module =
        load_project_binary(&config).unwrap_or_else(|e| panic!("load mvp_acceptance: {e}"));
    verify(&module).unwrap_or_else(|e| panic!("verify mvp_acceptance: {e}"));
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

fn scalar_u8(value: Value) -> Option<u8> {
    match value {
        Value::Scalar(ScalarValue::U8(v)) => Some(v),
        _ => None,
    }
}

fn scalar_i64(value: Value) -> Option<i64> {
    match value {
        Value::Scalar(ScalarValue::I64(v)) => Some(v),
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

fn main_locals_contain_u8(module: &phx_bytecode::BytecodeModule, expected: u8) {
    let capture = run_captured(module).unwrap_or_else(|e| panic!("run: {e}"));
    assert!(
        capture
            .main_locals
            .iter()
            .any(|v| scalar_u8(*v) == Some(expected)),
        "expected u8 {expected} in main locals: {:?}",
        capture.main_locals
    );
}

fn main_locals_contain_i64(module: &phx_bytecode::BytecodeModule, expected: i64) {
    let capture = run_captured(module).unwrap_or_else(|e| panic!("run: {e}"));
    assert!(
        capture
            .main_locals
            .iter()
            .any(|v| scalar_i64(*v) == Some(expected)),
        "expected s64 {expected} in main locals: {:?}",
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

#[test]
fn control_flow_loop_counter_reaches_ten() {
    let module = compile_fixture("control_flow.phx");
    main_locals_contain_i32(&module, 10);
}

#[test]
fn cast_width_sum_is_one_forty_two() {
    let module = compile_fixture("cast_width.phx");
    main_locals_contain_i64(&module, 142);
}

#[test]
fn match_int_selects_arm_value() {
    let module = compile_fixture("match_int.phx");
    main_locals_contain_i32(&module, 20);
}

#[test]
fn logical_short_circuit_ok_is_true() {
    let module = compile_fixture("logical.phx");
    main_locals_contain_bool(&module, true);
}

#[test]
fn slice_from_array_index_byte() {
    let module = compile_fixture("slice_from_array.phx");
    main_locals_contain_u8(&module, b'Y');
}

#[test]
fn modules_import_adds_imported_values() {
    let module_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/fixtures/modules");
    let module = compile_fixture_with_module_root("main.phx", &module_root);
    main_locals_contain_i32(&module, 3);
}

#[test]
fn mvp_acceptance_along_plus_pick_is_four() {
    let module = compile_mvp_acceptance();
    main_locals_contain_i32(&module, 4);
}

//! End-to-end semantic tests: verify computed values at exact `main` local slots.
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

fn scalar_i8(value: Value) -> Option<i8> {
    match value {
        Value::Scalar(ScalarValue::I8(v)) => Some(v),
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

fn scalar_f64(value: Value) -> Option<f64> {
    match value {
        Value::Scalar(ScalarValue::F64(v)) => Some(v),
        _ => None,
    }
}

fn assert_main_local_i32(module: &phx_bytecode::BytecodeModule, slot: usize, expected: i32) {
    let capture = run_captured(module).unwrap_or_else(|e| panic!("run: {e}"));
    let actual = capture
        .main_local(slot)
        .and_then(scalar_i32)
        .unwrap_or_else(|| panic!("slot {slot} not s32: {:?}", capture.main_locals));
    assert_eq!(actual, expected, "slot {slot}");
}

fn assert_main_local_i8(module: &phx_bytecode::BytecodeModule, slot: usize, expected: i8) {
    let capture = run_captured(module).unwrap_or_else(|e| panic!("run: {e}"));
    let actual = capture
        .main_local(slot)
        .and_then(scalar_i8)
        .unwrap_or_else(|| panic!("slot {slot} not s8: {:?}", capture.main_locals));
    assert_eq!(actual, expected, "slot {slot}");
}

fn assert_main_local_bool(module: &phx_bytecode::BytecodeModule, slot: usize, expected: bool) {
    let capture = run_captured(module).unwrap_or_else(|e| panic!("run: {e}"));
    let actual = capture
        .main_local(slot)
        .and_then(scalar_bool)
        .unwrap_or_else(|| panic!("slot {slot} not bool: {:?}", capture.main_locals));
    assert_eq!(actual, expected, "slot {slot}");
}

fn assert_main_local_u8(module: &phx_bytecode::BytecodeModule, slot: usize, expected: u8) {
    let capture = run_captured(module).unwrap_or_else(|e| panic!("run: {e}"));
    let actual = capture
        .main_local(slot)
        .and_then(scalar_u8)
        .unwrap_or_else(|| panic!("slot {slot} not u8: {:?}", capture.main_locals));
    assert_eq!(actual, expected, "slot {slot}");
}

fn assert_main_local_i64(module: &phx_bytecode::BytecodeModule, slot: usize, expected: i64) {
    let capture = run_captured(module).unwrap_or_else(|e| panic!("run: {e}"));
    let actual = capture
        .main_local(slot)
        .and_then(scalar_i64)
        .unwrap_or_else(|| panic!("slot {slot} not s64: {:?}", capture.main_locals));
    assert_eq!(actual, expected, "slot {slot}");
}

fn assert_main_local_f64(module: &phx_bytecode::BytecodeModule, slot: usize, expected: f64) {
    let capture = run_captured(module).unwrap_or_else(|e| panic!("run: {e}"));
    let actual = capture
        .main_local(slot)
        .and_then(scalar_f64)
        .unwrap_or_else(|| panic!("slot {slot} not f64: {:?}", capture.main_locals));
    assert!(
        (actual - expected).abs() < f64::EPSILON,
        "slot {slot}: expected {expected}, got {actual}"
    );
}

#[test]
fn sample_arithmetic_computes_sum() {
    let module = compile_fixture("sample.phx");
    assert_main_local_i32(&module, 2, 12);
    assert_main_local_bool(&module, 3, true);
}

#[test]
fn enum_match_extracts_payload() {
    let module = compile_fixture("enum_match.phx");
    assert_main_local_i32(&module, 2, 42);
}

#[test]
fn struct_method_sums_fields() {
    let module = compile_fixture("struct_method.phx");
    assert_main_local_i32(&module, 1, 7);
}

#[test]
fn struct_point_sums_via_function() {
    let module = compile_fixture("struct_point.phx");
    assert_main_local_i32(&module, 1, 7);
}

#[test]
fn trait_eq_method_returns_true() {
    let module = compile_fixture("trait_eq.phx");
    assert_main_local_bool(&module, 2, true);
}

#[test]
fn control_flow_loop_counter_reaches_ten() {
    let module = compile_fixture("control_flow.phx");
    assert_main_local_i32(&module, 0, 10);
}

#[test]
fn cast_width_sum_is_one_forty_two() {
    let module = compile_fixture("cast_width.phx");
    assert_main_local_i64(&module, 3, 142);
}

#[test]
fn match_int_selects_arm_value() {
    let module = compile_fixture("match_int.phx");
    assert_main_local_i32(&module, 2, 20);
}

#[test]
fn logical_short_circuit_ok_is_true() {
    let module = compile_fixture("logical.phx");
    assert_main_local_bool(&module, 4, true);
}

#[test]
fn slice_from_array_index_byte() {
    let module = compile_fixture("slice_from_array.phx");
    assert_main_local_u8(&module, 2, b'Y');
}

#[test]
fn modules_import_adds_imported_values() {
    let module_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/fixtures/modules");
    let module = compile_fixture_with_module_root("main.phx", &module_root);
    assert_main_local_i32(&module, 0, 3);
}

#[test]
fn mvp_acceptance_along_plus_pick_is_four() {
    let module = compile_mvp_acceptance();
    assert_main_local_i32(&module, 6, 4);
}

#[test]
fn factorial_computes_one_twenty() {
    let module = compile_fixture("factorial.phx");
    assert_main_local_i32(&module, 0, 120);
}

#[test]
fn given_enum_single_variant_binds_payload() {
    let module = compile_fixture("given_enum_single_variant.phx");
    assert_main_local_i32(&module, 1, 12);
}

#[test]
fn ref_local_derefs_to_ten() {
    let module = compile_fixture("ref_local.phx");
    assert_main_local_i32(&module, 2, 10);
}

#[test]
fn deref_ptr_reads_seventy_seven() {
    let module = compile_fixture("deref_ptr.phx");
    assert_main_local_u8(&module, 2, 77);
}

#[test]
fn byte_string_index_is_capital_b() {
    let module = compile_fixture("byte_string.phx");
    assert_main_local_u8(&module, 1, b'B');
}

#[test]
fn string_literal_index_is_lowercase_o() {
    let module = compile_fixture("string_literal.phx");
    assert_main_local_u8(&module, 2, b'o');
}

#[test]
fn byte_string_as_str_const_fold_index_is_lowercase_o() {
    let module = compile_fixture("byte_string_as_str.phx");
    assert_main_local_u8(&module, 3, b'o');
}

#[test]
fn primitives_width_sums_to_two_fifty_five() {
    let module = compile_fixture("primitives_width.phx");
    assert_main_local_i64(&module, 3, 255);
}

#[test]
fn primitives_float_mixed_width_sum() {
    let module = compile_fixture("primitives_float.phx");
    assert_main_local_f64(&module, 6, 145.75);
}

#[test]
fn primitives_i128_truncates_to_s8() {
    let module = compile_fixture("primitives_i128.phx");
    assert_main_local_i8(&module, 1, -24);
}

#[test]
fn compare_unary_and_relations_score() {
    let module = compile_fixture("compare_unary.phx");
    assert_main_local_i32(&module, 11, -4);
}

#[test]
fn mod_bitwise_ops_sum() {
    let module = compile_fixture("mod_bitwise.phx");
    assert_main_local_i32(&module, 5, -7);
}

#[test]
fn array_index_reads_middle_element() {
    let module = compile_fixture("array_index.phx");
    assert_main_local_i32(&module, 1, 20);
}

#[test]
fn tuple_lit_first_element() {
    let module = compile_fixture("tuple_lit.phx");
    assert_main_local_i32(&module, 1, 1);
}

#[test]
fn match_bool_true_arm() {
    let module = compile_fixture("match_bool.phx");
    assert_main_local_i32(&module, 2, 1);
}

#[test]
fn match_ident_wildcard_arm() {
    let module = compile_fixture("match_ident.phx");
    assert_main_local_i32(&module, 2, 20);
}

#[test]
fn struct_assign_updates_field() {
    let module = compile_fixture("struct_assign.phx");
    assert_main_local_i32(&module, 1, 5);
}

#[test]
fn enum_match_struct_extracts_payload() {
    let module = compile_fixture("enum_match_struct.phx");
    assert_main_local_i32(&module, 2, 42);
}

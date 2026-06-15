//! End-to-end semantic tests: verify computed values at exact `main` local slots.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_test::{
    ExpectedLocal, assert_main_locals, cli_modules_dir, compile_fixture, compile_fixture_module,
    force_build_project,
};

#[test]
fn sample_arithmetic_computes_sum() {
    let module = compile_fixture("sample.phx");
    assert_main_locals(
        &module,
        &[(2, ExpectedLocal::S32(12)), (3, ExpectedLocal::Bool(true))],
    );
}

#[test]
fn enum_match_extracts_payload() {
    assert_main_locals(
        &compile_fixture("enum_match.phx"),
        &[(2, ExpectedLocal::S32(42))],
    );
}

#[test]
fn struct_method_sums_fields() {
    assert_main_locals(
        &compile_fixture("struct_method.phx"),
        &[(1, ExpectedLocal::S32(7))],
    );
}

#[test]
fn struct_point_sums_via_function() {
    assert_main_locals(
        &compile_fixture("struct_point.phx"),
        &[(1, ExpectedLocal::S32(7))],
    );
}

#[test]
fn trait_eq_method_returns_true() {
    assert_main_locals(
        &compile_fixture("trait_eq.phx"),
        &[(2, ExpectedLocal::Bool(true))],
    );
}

#[test]
fn control_flow_loop_counter_reaches_ten() {
    assert_main_locals(
        &compile_fixture("control_flow.phx"),
        &[(0, ExpectedLocal::S32(10))],
    );
}

#[test]
fn cast_width_sum_is_one_forty_two() {
    assert_main_locals(
        &compile_fixture("cast_width.phx"),
        &[(3, ExpectedLocal::S64(142))],
    );
}

#[test]
fn match_int_selects_arm_value() {
    assert_main_locals(
        &compile_fixture("match_int.phx"),
        &[(2, ExpectedLocal::S32(20))],
    );
}

#[test]
fn logical_short_circuit_ok_is_true() {
    assert_main_locals(
        &compile_fixture("logical.phx"),
        &[(4, ExpectedLocal::Bool(true))],
    );
}

#[test]
fn slice_from_array_index_byte() {
    assert_main_locals(
        &compile_fixture("slice_from_array.phx"),
        &[(2, ExpectedLocal::U8(b'Y'))],
    );
}

#[test]
fn modules_import_adds_imported_values() {
    let module = compile_fixture_module("main.phx", &cli_modules_dir());
    assert_main_locals(&module, &[(0, ExpectedLocal::S32(3))]);
}

#[test]
fn mvp_acceptance_along_plus_pick_is_four() {
    let built = force_build_project("mvp_acceptance");
    assert_main_locals(&built.module, &[(6, ExpectedLocal::S32(4))]);
}

#[test]
fn factorial_computes_one_twenty() {
    assert_main_locals(
        &compile_fixture("factorial.phx"),
        &[(0, ExpectedLocal::S32(120))],
    );
}

#[test]
fn if_const_enum_single_variant_binds_payload() {
    assert_main_locals(
        &compile_fixture("if_const_enum_single_variant.phx"),
        &[(1, ExpectedLocal::S32(12))],
    );
}

#[test]
fn ref_local_derefs_to_ten() {
    assert_main_locals(
        &compile_fixture("ref_local.phx"),
        &[(2, ExpectedLocal::S32(10))],
    );
}

#[test]
fn deref_ptr_reads_seventy_seven() {
    assert_main_locals(
        &compile_fixture("deref_ptr.phx"),
        &[(2, ExpectedLocal::U8(77))],
    );
}

#[test]
fn heap_alloc_reads_seventy_seven_from_heap() {
    let _lock = phx_test::fixture_fs_lock();
    phx_test::require_cli_project("heap_alloc");
    let built = phx_test::force_build_project("heap_alloc");
    assert_main_locals(&built.module, &[(2, ExpectedLocal::U8(77))]);
}

#[test]
fn byte_string_index_is_capital_b() {
    assert_main_locals(
        &compile_fixture("byte_string.phx"),
        &[(1, ExpectedLocal::U8(b'B'))],
    );
}

#[test]
fn string_literal_index_is_lowercase_o() {
    assert_main_locals(
        &compile_fixture("string_literal.phx"),
        &[(2, ExpectedLocal::U8(b'o'))],
    );
}

#[test]
fn byte_string_as_str_const_fold_index_is_lowercase_o() {
    assert_main_locals(
        &compile_fixture("byte_string_as_str.phx"),
        &[(3, ExpectedLocal::U8(b'o'))],
    );
}

#[test]
fn primitives_width_sums_to_two_fifty_five() {
    assert_main_locals(
        &compile_fixture("primitives_width.phx"),
        &[(3, ExpectedLocal::S64(255))],
    );
}

#[test]
fn primitives_float_mixed_width_sum() {
    assert_main_locals(
        &compile_fixture("primitives_float.phx"),
        &[(6, ExpectedLocal::F64(145.75))],
    );
}

#[test]
fn primitives_i128_truncates_to_s8() {
    assert_main_locals(
        &compile_fixture("primitives_i128.phx"),
        &[(1, ExpectedLocal::S8(-24))],
    );
}

#[test]
fn primitives_u128_div_mod_above_i128_max() {
    const TOP: u128 = 1u128 << 127;
    const HALF: u128 = TOP / 2;
    const REM: u128 = TOP % 3;
    assert_main_locals(
        &compile_fixture("primitives_u128.phx"),
        &[
            (2, ExpectedLocal::U128(HALF)),
            (3, ExpectedLocal::U128(REM)),
            (4, ExpectedLocal::U128(HALF + REM)),
        ],
    );
}

#[test]
fn shift_width_mask_wrapping_shl() {
    assert_main_locals(
        &compile_fixture("shift_width_mask.phx"),
        &[(0, ExpectedLocal::U8(2)), (1, ExpectedLocal::U32(1))],
    );
}

#[test]
fn compare_unary_and_relations_score() {
    assert_main_locals(
        &compile_fixture("compare_unary.phx"),
        &[(11, ExpectedLocal::S32(-4))],
    );
}

#[test]
fn mod_bitwise_ops_sum() {
    assert_main_locals(
        &compile_fixture("mod_bitwise.phx"),
        &[(5, ExpectedLocal::S32(-7))],
    );
}

#[test]
fn array_index_reads_middle_element() {
    assert_main_locals(
        &compile_fixture("array_index.phx"),
        &[(1, ExpectedLocal::S32(20))],
    );
}

#[test]
fn tuple_lit_first_element() {
    assert_main_locals(
        &compile_fixture("tuple_lit.phx"),
        &[(1, ExpectedLocal::S32(1))],
    );
}

#[test]
fn match_bool_true_arm() {
    assert_main_locals(
        &compile_fixture("match_bool.phx"),
        &[(2, ExpectedLocal::S32(1))],
    );
}

#[test]
fn match_ident_wildcard_arm() {
    assert_main_locals(
        &compile_fixture("match_ident.phx"),
        &[(2, ExpectedLocal::S32(20))],
    );
}

#[test]
fn struct_assign_updates_field() {
    assert_main_locals(
        &compile_fixture("struct_assign.phx"),
        &[(1, ExpectedLocal::S32(5))],
    );
}

#[test]
fn enum_match_struct_extracts_payload() {
    assert_main_locals(
        &compile_fixture("enum_match_struct.phx"),
        &[(2, ExpectedLocal::S32(42))],
    );
}

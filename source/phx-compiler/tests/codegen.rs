//! Codegen integration tests.

use std::path::Path;

use phx_bytecode::verify;
use phx_bytecode::{BytecodeModule, ConstTag, Opcode};
use phx_compiler::{IrBinOp, IrInst, codegen, compile_source, lower};

#[test]
fn codegen_sample_round_trip_and_verify() {
    let source = include_str!("../../../tests/cli/fixtures/sample.phx");
    let unit = compile_source(source, Some(Path::new("sample.phx")))
        .unwrap_or_else(|e| panic!("compile: {e}"));
    let ir = lower(&unit.typed);
    let module = codegen(&ir, &unit.typed.layout);

    assert_eq!(module.functions.functions.len(), 2);
    assert!(!module.code.is_empty());
    assert!(module.constants.entries.len() >= 2);

    let main_id = module.header.entry_function_id;
    let main_rec = module
        .functions
        .functions
        .iter()
        .find(|f| f.function_id == main_id)
        .expect("main function record");
    assert_eq!(main_rec.arity, 0);

    let bytes = module.encode();
    let decoded = BytecodeModule::decode(&bytes).expect("decode");
    verify(&decoded).expect("verify");

    let add_rec = module
        .functions
        .functions
        .iter()
        .find(|f| f.function_id != main_id)
        .expect("add");
    assert_eq!(add_rec.arity, 2);

    let add_code = &module.code
        [add_rec.code_offset as usize..add_rec.code_offset as usize + add_rec.code_len as usize];
    assert!(add_code.contains(&Opcode::Add.as_u8()));
}

#[test]
fn codegen_constants_include_sample_literals() {
    let source = include_str!("../../../tests/cli/fixtures/sample.phx");
    let unit = compile_source(source, None).unwrap();
    let module = codegen(&lower(&unit.typed), &unit.typed.layout);

    let mut has_ten = false;
    let mut has_two = false;
    for entry in &module.constants.entries {
        if entry.tag == ConstTag::SignedInt && entry.payload.len() >= 8 {
            let v = i64::from_le_bytes(entry.payload[0..8].try_into().unwrap());
            if v == 10 {
                has_ten = true;
            }
            if v == 2 {
                has_two = true;
            }
        }
    }
    assert!(has_ten && has_two);
}

#[test]
fn continue_program_runs_on_vm() {
    let source =
        "main :: () => { var i: s32 = 0; loop { i = i + 1; if 3 > (i) { continue; } break; }; };";
    let unit = compile_source(source, None).unwrap();
    let module = codegen(&lower(&unit.typed), &unit.typed.layout);
    verify(&module).expect("verify continue program");
}

#[test]
fn lower_logical_short_circuit_emits_jump_if() {
    let source = "main :: () => { const a: bool = true && false; const b: bool = true || false; const c: bool = a || b; const _ = c; };";
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed);
    let mut jump_if_count = 0u32;
    for f in &ir.functions {
        for block in &f.blocks {
            for inst in &block.insts {
                if matches!(inst, IrInst::JumpIf { .. }) {
                    jump_if_count += 1;
                }
            }
        }
    }
    assert!(
        jump_if_count >= 2,
        "&& and || should lower to at least two JumpIf terminators"
    );
}

#[test]
fn lower_match_emits_eq_and_jump_if() {
    let source = "main :: () => { var i: s32 = 1; const x: s32 = { match (i) { 0 => 10; _ => 20; } }; const _ = x; };";
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed);
    let mut eq_count = 0u32;
    let mut jump_if_count = 0u32;
    for f in &ir.functions {
        for block in &f.blocks {
            for inst in &block.insts {
                if matches!(
                    inst,
                    IrInst::BinOp {
                        op: IrBinOp::Eq,
                        ..
                    }
                ) {
                    eq_count += 1;
                }
                if matches!(inst, IrInst::JumpIf { .. }) {
                    jump_if_count += 1;
                }
            }
        }
    }
    assert!(eq_count >= 1, "literal match arm should compare with Eq");
    assert!(jump_if_count >= 1, "match should branch with JumpIf");
}

#[test]
fn codegen_enum_match_verifies() {
    let source = include_str!("../../../tests/cli/fixtures/enum_match.phx");
    let unit = compile_source(source, None).unwrap();
    let module = codegen(&lower(&unit.typed), &unit.typed.layout);
    verify(&module).expect("enum_match bytecode should verify");
}

#[test]
fn codegen_struct_point_emits_make_struct() {
    let source = include_str!("../../../tests/cli/fixtures/struct_point.phx");
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed);
    let has_make = ir
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .any(|i| matches!(i, IrInst::MakeStruct { .. }));
    assert!(has_make, "struct literal should emit MakeStruct");
    let has_get = ir
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .any(|i| matches!(i, IrInst::GetField { .. }));
    assert!(has_get, "field read should emit GetField");
    let module = codegen(&ir, &unit.typed.layout);
    verify(&module).expect("struct_point bytecode should verify");
}

#[test]
fn assign_to_var_emits_store_local() {
    let source = "main :: () => { var i: s32 = 0; i = i + 1; const _ = i; };";
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed);
    let stores = ir
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .filter(|inst| matches!(inst, IrInst::StoreLocal { .. }))
        .count();
    assert!(stores >= 2, "var init and assign should both StoreLocal");
}

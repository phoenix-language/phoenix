//! Codegen tests using on-disk fixtures (migrated from phx-compiler/tests/codegen.rs).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_bytecode::Opcode;
use phx_bytecode::{ScalarValue, verify};
use phx_compiler::unstable::{IrInst, codegen, lower};
use phx_test::cli_project_main;
use phx_vm::{Value, run_captured};

#[test]
fn codegen_heap_slice_emits_make_slice_from_ptr_opcode() {
    let path = cli_project_main("heap_slice");
    let unit = phx_compiler::check_file(&path).expect("check heap_slice");
    let ir = lower(&unit.typed).expect("lower heap_slice");
    assert!(
        ir.functions.iter().any(|f| {
            f.blocks.iter().any(|b| {
                b.insts
                    .iter()
                    .any(|s| matches!(&s.inst, IrInst::MakeSliceFromPtr { .. }))
            })
        }),
        "expected IrInst::MakeSliceFromPtr in heap_slice"
    );
    let module = codegen(&ir, &unit.typed).expect("codegen heap_slice");
    assert!(
        module.code.contains(&Opcode::MakeSliceFromPtr.as_u8()),
        "expected MAKE_SLICE_FROM_PTR opcode in heap_slice bytecode"
    );
    verify(&module).expect("verify heap_slice");
}

#[test]
fn codegen_heap_slice_store_emits_index_store_opcode() {
    let path = cli_project_main("heap_slice_store");
    let unit = phx_compiler::check_file(&path).expect("check heap_slice_store");
    let ir = lower(&unit.typed).expect("lower heap_slice_store");
    assert!(
        ir.functions.iter().any(|f| {
            f.blocks.iter().any(|b| {
                b.insts
                    .iter()
                    .any(|s| matches!(&s.inst, IrInst::IndexStore { .. }))
            })
        }),
        "expected IrInst::IndexStore in heap_slice_store"
    );
    let module = codegen(&ir, &unit.typed).expect("codegen heap_slice_store");
    assert!(
        module.code.contains(&Opcode::IndexStore.as_u8()),
        "expected INDEX_STORE opcode in heap_slice_store bytecode"
    );
    verify(&module).expect("verify heap_slice_store");
}

#[test]
fn codegen_heap_slice_nested_index_emits_index_store_opcode() {
    let path = cli_project_main("heap_slice_nested_index");
    let unit = phx_compiler::check_file(&path).expect("check heap_slice_nested_index");
    let ir = lower(&unit.typed).expect("lower heap_slice_nested_index");
    let module = codegen(&ir, &unit.typed).expect("codegen heap_slice_nested_index");
    assert!(
        module.code.contains(&Opcode::IndexStore.as_u8()),
        "expected INDEX_STORE opcode in heap_slice_nested_index bytecode"
    );
    verify(&module).expect("verify heap_slice_nested_index");
}

#[test]
fn codegen_heap_slice_store_run_captured_roundtrip() {
    let path = cli_project_main("heap_slice_store");
    let unit = phx_compiler::check_file(&path).expect("check heap_slice_store");
    let ir = lower(&unit.typed).expect("lower");
    let module = codegen(&ir, &unit.typed).expect("codegen");
    let verified = verify(&module).expect("verify");
    let capture = run_captured(verified).expect("run");
    let v = capture
        .main_local(3)
        .and_then(|val| match val {
            Value::Scalar(ScalarValue::U8(n)) => Some(n),
            _ => None,
        })
        .unwrap_or_else(|| panic!("slot 3 u8: {:?}", capture.main_locals));
    assert_eq!(v, 42);
}

#[test]
fn codegen_dynamic_array_grow_s32_index_store_operands() {
    let path = cli_project_main("dynamic_array_grow");
    let unit = phx_compiler::check_file(&path).expect("check dynamic_array_grow");
    let ir = lower(&unit.typed).expect("lower dynamic_array_grow");
    let module = codegen(&ir, &unit.typed).expect("codegen dynamic_array_grow");
    verify(&module).expect("verify dynamic_array_grow");
    let verified = verify(&module).expect("verify");
    let capture = run_captured(verified).expect("run dynamic_array_grow");
    let sum = capture
        .main_local(10)
        .and_then(|val| match val {
            Value::Scalar(ScalarValue::I32(n)) => Some(n),
            _ => None,
        })
        .expect("slot 10 should be check_sum s32");
    assert_eq!(sum, 36, "grow fixture sum of 1..=8");
}

#[test]
fn codegen_heap_dealloc_emits_free_opcode() {
    let path = cli_project_main("heap_dealloc");
    let unit = phx_compiler::check_file(&path).expect("check heap_dealloc");
    let ir = lower(&unit.typed).expect("lower heap_dealloc");
    assert!(
        ir.functions.iter().any(|f| {
            f.blocks
                .iter()
                .any(|b| b.insts.iter().any(|s| matches!(&s.inst, IrInst::Free)))
        }),
        "expected IrInst::Free in heap_dealloc"
    );
    let module = codegen(&ir, &unit.typed).expect("codegen heap_dealloc");
    assert!(
        module.code.contains(&Opcode::Free.as_u8()),
        "expected FREE opcode in heap_dealloc bytecode"
    );
    verify(&module).expect("verify heap_dealloc");
}

#[test]
fn codegen_heap_alloc_emits_alloc_opcode() {
    let path = cli_project_main("heap_alloc");
    let unit = phx_compiler::check_file(&path).expect("check heap_alloc");
    let ir = lower(&unit.typed).expect("lower heap_alloc");
    assert!(
        ir.functions.iter().any(|f| {
            f.blocks.iter().any(|b| {
                b.insts
                    .iter()
                    .any(|s| matches!(&s.inst, IrInst::Alloc { .. }))
            })
        }),
        "expected IrInst::Alloc in heap_alloc"
    );
    assert!(
        ir.functions.iter().any(|f| {
            f.blocks.iter().any(|b| {
                b.insts
                    .iter()
                    .any(|s| matches!(&s.inst, IrInst::PtrStore { .. }))
            })
        }),
        "expected IrInst::PtrStore in heap_alloc"
    );
    let module = codegen(&ir, &unit.typed).expect("codegen heap_alloc");
    assert!(
        module.code.contains(&Opcode::Alloc.as_u8()),
        "expected ALLOC opcode in heap_alloc bytecode"
    );
    verify(&module).expect("verify heap_alloc");
}

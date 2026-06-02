//! Lowering integration tests.

use std::path::Path;

use phx_compiler::{compile_source, lower, IrBinOp, IrInst};

#[test]
fn lower_sample_produces_ir() {
    let source = include_str!("../../../tests/cli/fixtures/sample.phx");
    let unit = compile_source(source, Some(Path::new("sample.phx")))
        .unwrap_or_else(|e| panic!("compile sample.phx: {e}"));
    assert!(!unit.typed.functions.is_empty());

    let ir = lower(&unit.typed);
    assert_eq!(ir.functions.len(), 2, "add and main");
    assert!(ir.entry.is_some());

    let has_add = |inst: &IrInst| {
        matches!(
            inst,
            IrInst::BinOp {
                op: IrBinOp::Add,
                ..
            }
        )
    };
    let has_call = |inst: &IrInst| matches!(inst, IrInst::Call { .. });
    let has_store = |inst: &IrInst| matches!(inst, IrInst::StoreLocal { .. });
    let has_const = |inst: &IrInst| matches!(inst, IrInst::Const { .. });
    let has_jump_if = |inst: &IrInst| matches!(inst, IrInst::JumpIf { .. });

    let mut any_add = false;
    let mut any_call = false;
    let mut store_count = 0u32;
    let mut any_const = false;
    let mut any_jump_if = false;
    let mut max_locals = 0u32;

    for f in &ir.functions {
        max_locals = max_locals.max(f.local_count);
        for block in &f.blocks {
            for inst in &block.insts {
                any_add |= has_add(inst);
                any_call |= has_call(inst);
                any_const |= has_const(inst);
                any_jump_if |= has_jump_if(inst);
                if has_store(inst) {
                    store_count += 1;
                }
            }
        }
    }

    assert!(any_add, "expected Add in add()");
    assert!(any_call, "expected Call add(base, step) in main");
    assert!(any_const, "expected literal Const instructions");
    assert!(any_jump_if, "expected if expr JumpIf in main");
    assert!(store_count >= 4, "const bindings should StoreLocal");
    assert!(max_locals >= 5, "main has params/locals for base, step, sum, ok, _");
}

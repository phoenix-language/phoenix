//! Lowering integration tests.

use std::path::Path;

use phx_compiler::{IrBinOp, IrInst, compile_source, lower};

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
    assert!(
        max_locals >= 5,
        "main has params/locals for base, step, sum, ok, _"
    );
}

#[test]
fn lower_control_flow_emits_loops() {
    let source = include_str!("../../../tests/cli/fixtures/control_flow.phx");
    let unit = compile_source(source, Some(Path::new("control_flow.phx")))
        .unwrap_or_else(|e| panic!("compile control_flow.phx: {e}"));
    let ir = lower(&unit.typed);

    let mut jump_count = 0u32;
    let mut jump_if_count = 0u32;
    for f in &ir.functions {
        for block in &f.blocks {
            for inst in &block.insts {
                if matches!(inst, IrInst::Jump { .. }) {
                    jump_count += 1;
                }
                if matches!(inst, IrInst::JumpIf { .. }) {
                    jump_if_count += 1;
                }
            }
        }
    }
    assert!(jump_count >= 4, "while/loop/break/continue need Jump");
    assert!(jump_if_count >= 2, "while and if-in-loop need JumpIf");
}

#[test]
fn continue_merge_block_has_no_loop_back_edge() {
    let source = "main :: () => { var i: s32 = 0; loop { i = i + 1; if 3 > (i) { continue; } break; }; };";
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed);
    let main = ir
        .functions
        .iter()
        .find(|f| Some(f.def) == ir.entry)
        .expect("main");
    let merge = main
        .blocks
        .iter()
        .find(|b| {
            b.insts
                .iter()
                .any(|i| matches!(i, IrInst::Jump { target: 2 }))
        })
        .expect("merge with break");
    assert!(
        !merge
            .insts
            .iter()
            .any(|i| matches!(i, IrInst::Jump { target: 1 })),
        "loop back-edge must not be emitted on the if merge block"
    );
    let header = &main.blocks[1];
    assert!(
        header.insts.last().is_some_and(|i| matches!(i, IrInst::Jump { target: 1 })),
        "loop header block should end with back-edge jump"
    );
}
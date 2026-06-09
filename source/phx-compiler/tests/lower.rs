//! Lowering integration tests.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use phx_compiler::{IrBinOp, IrInst, compile_source, lower};

#[test]
fn lower_sample_produces_ir() {
    let source = include_str!("../../../tests/cli/fixtures/sample.phx");
    let unit = compile_source(source, Some(Path::new("sample.phx")))
        .unwrap_or_else(|e| panic!("compile sample.phx: {e}"));
    assert!(!unit.typed.functions.is_empty());

    let ir = lower(&unit.typed).expect("lower");
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
    let ir = lower(&unit.typed).expect("lower");

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
fn loop_back_edge_on_body_tail_not_header() {
    let source =
        "main :: () => { var i: s32 = 0; loop { i = i + 1; if 3 > (i) { continue; } break; }; };";
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed).expect("lower");
    let main = ir
        .functions
        .iter()
        .find(|f| Some(f.def) == ir.entry)
        .expect("main");
    let header = &main.blocks[1];
    assert!(
        !header
            .insts
            .iter()
            .any(|i| matches!(i, IrInst::Jump { target: 1 })),
        "loop header must not contain the back-edge jump"
    );
    let tail = main
        .blocks
        .iter()
        .find(|b| {
            b.insts
                .iter()
                .any(|i| matches!(i, IrInst::Jump { target: 1 }))
                && !b.insts.iter().any(|i| matches!(i, IrInst::JumpIf { .. }))
        })
        .expect("body tail merge block should jump back to header");
    assert!(
        tail.insts
            .last()
            .is_some_and(|i| matches!(i, IrInst::Jump { target: 1 })),
        "back-edge must be the tail block terminator"
    );
}

#[test]
fn explicit_return_emits_single_return() {
    let source = "main :: () => { return; };";
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed).expect("lower");
    let main = ir
        .functions
        .iter()
        .find(|f| Some(f.def) == ir.entry)
        .expect("main");
    let return_count: u32 = main
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter(|i| matches!(i, IrInst::Return { .. }))
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    assert_eq!(
        return_count, 1,
        "explicit return must not get a second synthetic Return"
    );
}

#[test]
fn const_fold_byte_array_as_str_emits_make_str_not_make_array() {
    let source = "main :: () => { const arr = b\"hi\"; const s: str = arr as str; const _ = s; };";
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed).expect("lower");
    let main = ir
        .functions
        .iter()
        .find(|f| Some(f.def) == ir.entry)
        .expect("main");
    let make_str = main
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .any(|i| matches!(i, IrInst::MakeStr { .. }));
    let make_array_for_cast = main
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter(|i| matches!(i, IrInst::MakeArray { .. }))
        .count();
    assert!(make_str, "const-folded arr as str should emit MakeStr");
    assert_eq!(
        make_array_for_cast, 1,
        "only the arr initializer should MakeArray, not the cast"
    );
}

#[test]
fn lower_generic_fn_emits_call() {
    let source = include_str!("../../../tests/cli/fixtures/generic_fn.phx");
    let path = Path::new("generic_fn.phx");
    let unit = phx_compiler::check_file(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/generic_fn.phx"),
    )
    .unwrap_or_else(|e| panic!("check_file generic_fn.phx: {e}"));
    let ir = lower(&unit.typed).expect("lower");
    assert!(
        ir.functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.insts)
            .any(|i| matches!(i, IrInst::Call { .. })),
        "expected Call in main"
    );
    let _ = source;
    let _ = path;
}

#[test]
fn lower_generic_struct_emits_make_struct() {
    let source = include_str!("../../../tests/cli/fixtures/generic_struct.phx");
    let unit = compile_source(source, Some(Path::new("generic_struct.phx")))
        .unwrap_or_else(|e| panic!("compile generic_struct.phx: {e}"));
    let ir = lower(&unit.typed).expect("lower");
    assert!(
        ir.functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.insts)
            .any(|i| matches!(i, IrInst::MakeStruct { .. })),
        "expected MakeStruct for generic struct literal"
    );
}

#[test]
fn lower_dual_generic_fn_instantiation_emits_three_functions() {
    let source = "id :: <t> (x: t) => t { x }; main :: () => { const a: s32 = id :: <s32> (1); const b: bool = id :: <bool> (true); const _ = a; };";
    let unit = compile_source(source, None).expect("compile dual generic fn");
    let ir = lower(&unit.typed).expect("lower");
    assert_eq!(
        ir.functions.len(),
        3,
        "expected two monomorphized specials plus main"
    );
    let call_count = ir
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .filter(|i| matches!(i, IrInst::Call { .. }))
        .count();
    assert!(
        call_count >= 2,
        "main should call both specialized id functions"
    );
}

#[test]
fn lower_generic_enum_match_emits_match_tag_with_specialized_type_id() {
    let source = "Opt :: <t> enum { None, Some(t), }; main :: () => { const x = Some :: <s32> (1); const n: s32 = match x { None => 0; Some(v) => v; }; const _ = n; };";
    let unit = compile_source(source, None).expect("compile generic enum match");
    let typed = &unit.typed;
    let expected_type_id = typed
        .layout
        .specialized_type_ids
        .values()
        .next()
        .copied()
        .expect("monomorphized enum type_id");
    let ir = lower(typed).expect("lower");
    assert!(
        ir.functions.iter().any(|f| {
            f.blocks.iter().any(|b| {
                b.insts.iter().any(|i| {
                    matches!(
                        i,
                        IrInst::MatchTag {
                            type_id,
                            variant_tag: 1,
                            ..
                        } if *type_id == expected_type_id
                    )
                })
            })
        }),
        "expected MatchTag with specialized enum type_id {expected_type_id}"
    );
}

#[test]
fn lower_heap_slice_emits_make_slice_from_ptr_not_call() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/heap_slice/src/main.phx");
    if !path.is_file() {
        return;
    }
    let unit = phx_compiler::check_file(&path).expect("typecheck heap_slice");
    let ir = lower(&unit.typed).expect("lower heap_slice");
    let has_make = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts
                .iter()
                .any(|i| matches!(i, IrInst::MakeSliceFromPtr { .. }))
        })
    });
    let slice_def = unit.typed.intrinsic_kernel.slice_from_raw_parts;
    let has_call = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts.iter().any(|i| {
                if let IrInst::Call { callee, .. } = i {
                    slice_def == Some(*callee)
                } else {
                    false
                }
            })
        })
    });
    assert!(
        has_make,
        "expected MakeSliceFromPtr IR for slice_from_raw_parts"
    );
    assert!(
        !has_call,
        "slice_from_raw_parts must not lower to IrInst::Call"
    );
}

#[test]
fn lower_heap_alloc_emits_alloc_not_call() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/heap_alloc/src/main.phx");
    if !path.is_file() {
        return;
    }
    let unit = phx_compiler::check_file(&path).expect("typecheck heap_alloc");
    let ir = lower(&unit.typed).expect("lower heap_alloc");
    let has_alloc = ir.functions.iter().any(|f| {
        f.blocks
            .iter()
            .any(|b| b.insts.iter().any(|i| matches!(i, IrInst::Alloc { .. })))
    });
    let has_alloc_call = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts.iter().any(|i| {
                if let IrInst::Call { callee, .. } = i {
                    unit.typed.intrinsic_kernel.alloc_bytes == Some(*callee)
                } else {
                    false
                }
            })
        })
    });
    assert!(has_alloc, "expected Alloc IR for alloc_bytes");
    assert!(
        !has_alloc_call,
        "alloc_bytes must not lower to IrInst::Call"
    );
}

#[test]
fn lower_std_try_emits_question_mark_unwrap() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/std_try/src/main.phx");
    if !path.is_file() {
        return;
    }
    let unit = phx_compiler::check_file(&path).expect("typecheck std_try");
    let ir = lower(&unit.typed).expect("lower std_try");
    let interner = &unit.typed.resolved.interner;
    let has_try_lower = ir.functions.iter().any(|f| {
        let name = unit
            .typed
            .resolved
            .defs
            .get(f.def.index() as usize)
            .map(|d| interner.resolve(d.name));
        name == Some("read_config")
            && f.blocks.iter().any(|b| {
                b.insts
                    .iter()
                    .any(|i| matches!(i, IrInst::MatchTag { .. } | IrInst::GetField { .. }))
            })
    });
    assert!(
        has_try_lower,
        "read_config should lower ? via MatchTag/GetField"
    );
}

#[test]
fn lower_fn_pointer_emits_make_fn_ptr_and_call_indirect() {
    let source = "double :: (x: s32) => s32 { x + x }; apply :: (f: :: (s32) => s32, x: s32) => s32 { f(x) }; main :: () => { const n: s32 = apply(double, 3); const _ = n; };";
    let unit = compile_source(source, None).expect("compile fn pointer program");
    let ir = lower(&unit.typed).expect("lower");
    let has_make = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts
                .iter()
                .any(|i| matches!(i, IrInst::MakeFnPtr { .. }))
        })
    });
    let has_indirect = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts
                .iter()
                .any(|i| matches!(i, IrInst::CallIndirect { .. }))
        })
    });
    assert!(has_make, "expected MakeFnPtr when passing function by name");
    assert!(has_indirect, "expected CallIndirect for f(x)");
}

#[test]
fn lower_drop_emits_drop_local_before_return() {
    let source = r"
Drop :: trait { drop :: (self) => (); };
Wrapper :: struct {};
Wrapper :: impl :: Drop { drop :: (self) => () {}; };
main :: () => { { const w = Wrapper {}; } };
";
    let unit = compile_source(source, None).expect("compile");
    let ir = lower(&unit.typed).expect("lower");
    let main = ir
        .functions
        .iter()
        .find(|f| Some(f.def) == ir.entry)
        .expect("main");
    let has_drop = main.blocks.iter().any(|b| {
        b.insts
            .iter()
            .any(|i| matches!(i, IrInst::DropLocal { .. }))
    });
    assert!(has_drop, "expected DropLocal in main for scope-exit glue");
}

#[test]
fn lower_for_in_emits_iterator_protocol() {
    let source = include_str!("../../../tests/cli/fixtures/for_in.phx");
    let unit = compile_source(source, Some(Path::new("for_in.phx")))
        .unwrap_or_else(|e| panic!("compile for_in.phx: {e}"));
    let ir = lower(&unit.typed).expect("lower");
    let main = ir
        .functions
        .iter()
        .find(|f| Some(f.def) == ir.entry)
        .expect("main");
    let insts: Vec<_> = main.blocks.iter().flat_map(|b| &b.insts).collect();
    assert!(
        insts.iter().any(|i| matches!(i, IrInst::Call { .. })),
        "expected into_iter / next Call"
    );
    assert!(
        insts
            .iter()
            .any(|i| matches!(i, IrInst::AddressOfLocal { .. })),
        "expected AddressOfLocal for &mut __iter.next()"
    );
    assert!(
        insts.iter().any(|i| matches!(i, IrInst::Jump { .. })),
        "expected loop Jump"
    );
    assert!(
        insts.iter().any(|i| matches!(i, IrInst::JumpIf { .. })),
        "expected if-const Some JumpIf"
    );
}

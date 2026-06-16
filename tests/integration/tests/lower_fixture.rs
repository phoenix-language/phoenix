//! Lowering tests using on-disk fixtures (migrated from phx-compiler/tests/lower.rs).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_compiler::unstable::{IrInst, lower};
use phx_test::cli_project_main;

#[test]
fn lower_trait_default_emits_inherited_method() {
    let path = cli_project_main("trait_default");
    let unit = phx_compiler::check_file(&path).expect("typecheck trait_default");
    assert!(
        !unit.typed.inherited_trait_methods.is_empty(),
        "expected inherited trait default methods"
    );
    let ir = lower(&unit.typed).expect("lower trait_default");
    let has_call = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts
                .iter()
                .any(|s| matches!(&s.inst, IrInst::Call { .. }))
        })
    });
    assert!(
        has_call,
        "expected Call to inherited Counter::zero in lowered IR"
    );
}

#[test]
fn lower_heap_slice_emits_make_slice_from_ptr_not_call() {
    let path = cli_project_main("heap_slice");
    let unit = phx_compiler::check_file(&path).expect("typecheck heap_slice");
    let ir = lower(&unit.typed).expect("lower heap_slice");
    let has_make = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts
                .iter()
                .any(|s| matches!(&s.inst, IrInst::MakeSliceFromPtr { .. }))
        })
    });
    let slice_def = unit.typed.lang_items.slice_from_raw_parts;
    let has_call = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts.iter().any(|s| {
                if let IrInst::Call { callee, .. } = &s.inst {
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
fn lower_heap_slice_store_emits_index_store() {
    let path = cli_project_main("heap_slice_store");
    let unit = phx_compiler::check_file(&path).expect("typecheck heap_slice_store");
    let ir = lower(&unit.typed).expect("lower heap_slice_store");
    let has_index_store = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts
                .iter()
                .any(|s| matches!(&s.inst, IrInst::IndexStore { .. }))
        })
    });
    assert!(
        has_index_store,
        "expected IrInst::IndexStore in heap_slice_store"
    );
}

#[test]
fn lower_heap_slice_nested_index_store_emits_index_store() {
    let path = cli_project_main("heap_slice_nested_index");
    let unit = phx_compiler::check_file(&path).expect("typecheck heap_slice_nested_index");
    let ir = lower(&unit.typed).expect("lower heap_slice_nested_index");
    let has_index_store = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts
                .iter()
                .any(|s| matches!(&s.inst, IrInst::IndexStore { .. }))
        })
    });
    let has_call_before_store = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            let mut saw_call = false;
            for spanned in &b.insts {
                if matches!(&spanned.inst, IrInst::Call { .. }) {
                    saw_call = true;
                }
                if saw_call && matches!(&spanned.inst, IrInst::IndexStore { .. }) {
                    return true;
                }
            }
            false
        })
    });
    assert!(
        has_index_store,
        "expected IrInst::IndexStore in heap_slice_nested_index"
    );
    assert!(
        has_call_before_store,
        "expected call to index fn before IndexStore in heap_slice_nested_index"
    );
}

#[test]
fn lower_dynamic_array_grow_emits_index_store_for_s32_slice() {
    let path = cli_project_main("dynamic_array_grow");
    let unit = phx_compiler::check_file(&path).expect("typecheck dynamic_array_grow");
    let ir = lower(&unit.typed).expect("lower dynamic_array_grow");
    let mut make_elem_kinds = Vec::new();
    let mut store_kinds = Vec::new();
    for f in &ir.functions {
        for b in &f.blocks {
            for s in &b.insts {
                if let IrInst::MakeSliceFromPtr { elem_kind } = &s.inst {
                    make_elem_kinds.push(*elem_kind);
                }
                if let IrInst::IndexStore {
                    prim_kind, signed, ..
                } = &s.inst
                {
                    store_kinds.push((*prim_kind, *signed));
                }
            }
        }
    }
    assert!(
        make_elem_kinds.contains(&phx_bytecode::PrimitiveKind::S32.as_u8()),
        "expected S32 elem_kind in MakeSliceFromPtr, got {make_elem_kinds:?}"
    );
    assert!(
        store_kinds.contains(&(phx_bytecode::PrimitiveKind::S32.as_u8(), 1)),
        "expected S32 signed IndexStore, got {store_kinds:?}"
    );
}

#[test]
fn lower_heap_dealloc_emits_free_not_call() {
    let path = cli_project_main("heap_dealloc");
    let unit = phx_compiler::check_file(&path).expect("typecheck heap_dealloc");
    let ir = lower(&unit.typed).expect("lower heap_dealloc");
    let has_free = ir.functions.iter().any(|f| {
        f.blocks
            .iter()
            .any(|b| b.insts.iter().any(|s| matches!(&s.inst, IrInst::Free)))
    });
    let has_dealloc_call = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts.iter().any(|s| {
                if let IrInst::Call { callee, .. } = &s.inst {
                    unit.typed.lang_items.dealloc_bytes == Some(*callee)
                } else {
                    false
                }
            })
        })
    });
    assert!(has_free, "expected Free IR for dealloc_bytes");
    assert!(
        !has_dealloc_call,
        "dealloc_bytes must not lower to IrInst::Call"
    );
}

#[test]
fn lower_heap_alloc_emits_alloc_not_call() {
    let path = cli_project_main("heap_alloc");
    let unit = phx_compiler::check_file(&path).expect("typecheck heap_alloc");
    let ir = lower(&unit.typed).expect("lower heap_alloc");
    let has_alloc = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts
                .iter()
                .any(|s| matches!(&s.inst, IrInst::Alloc { .. }))
        })
    });
    let has_alloc_call = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts.iter().any(|s| {
                if let IrInst::Call { callee, .. } = &s.inst {
                    unit.typed.lang_items.alloc_bytes == Some(*callee)
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
    let path = cli_project_main("std_try");
    let unit = phx_compiler::check_file(&path).expect("typecheck std_try");
    let ir = lower(&unit.typed).expect("lower std_try");
    let interner = &unit.typed.resolved.interner;
    let has_try_lower = ir.functions.iter().any(|f| {
        let Some(def) = unit.typed.resolved.defs.get(f.def.index() as usize) else {
            return false;
        };
        interner.resolves_to(def.name, "read_config")
            && f.blocks.iter().any(|b| {
                b.insts
                    .iter()
                    .any(|s| matches!(&s.inst, IrInst::MatchTag { .. } | IrInst::GetField { .. }))
            })
    });
    assert!(
        has_try_lower,
        "read_config should lower ? via MatchTag/GetField"
    );
}

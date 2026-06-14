//! AST-to-IR lowering.
#![allow(
    clippy::collapsible_if,
    clippy::match_same_arms,
    clippy::needless_pass_by_value,
    clippy::manual_let_else
)]
//!
//! Consumes a [`TypedProgram`](crate::typeck::TypedProgram) and produces an [`IrModule`](crate::ir::IrModule).
//! Does not read source text or build types — type checking must run first.
//!
//! Short-circuit `&&` and `||` lower to `JumpIf` chains (see `lower_short_circuit_bool` in `expr.rs`).
//! Merge blocks for `if`/`match` expressions rely on balanced stack depth per [`IrModule`](crate::ir::IrModule) invariants.
//!
//! ## Module map
//!
//! - [`func`] — top-level and impl function bodies
//! - [`expr`] — expression trees → instruction streams
//! - [`stmt`] — statements, bindings, control flow

mod ctx;
mod drop_glue;
mod expr;
mod func;
mod stmt;

use crate::ir::{IrConst, IrFunction, IrInst, IrModule};
use crate::typeck::TypedProgram;
use phx_diagnostics::{LowerBag, LowerResult};
use std::collections::{HashMap, HashSet};

/// Lowers `typed` to IR.
///
/// Walks typed AST nodes in the same expression order as typeck, uses
/// [`TypedProgram::functions`](crate::typeck::TypedProgram::functions) for [`LocalSlot`](crate::typeck::LocalSlot)
/// indices, and reads expression types from [`TypedProgram::expr_types`](crate::typeck::TypedProgram::expr_types).
///
/// # Errors
///
/// Returns [`LowerBag`] on internal invariant violations (e.g. unresolved call callee).
pub fn lower(typed: &TypedProgram) -> LowerResult<IrModule> {
    let mut constants = Vec::new();
    let mut bag = LowerBag::new();
    let functions = func::lower_functions(typed, &mut constants, &mut bag)?;
    Ok(IrModule {
        functions,
        entry: typed.entry,
        constants,
    })
}

/// Lowers only functions defined in `module_id` (slice of a full [`lower`] result).
///
/// # Errors
///
/// Same as [`lower`].
pub fn lower_module(typed: &TypedProgram, module_id: u32) -> LowerResult<IrModule> {
    let full = lower(typed)?;
    let mut functions: Vec<IrFunction> = full
        .functions
        .iter()
        .filter(|f| {
            typed
                .resolved
                .defs
                .get(f.def.index() as usize)
                .is_some_and(|d| d.module == module_id)
        })
        .cloned()
        .collect();
    let entry = typed.entry.filter(|&main| {
        typed
            .resolved
            .defs
            .get(main.index() as usize)
            .is_some_and(|d| d.module == module_id)
    });
    let constants = localize_module_constants(&mut functions, &full.constants);
    Ok(IrModule {
        functions,
        entry,
        constants,
    })
}

/// Keeps only literals referenced by `functions` and rewrites IR indices to a dense range.
#[allow(clippy::too_many_lines)]
fn localize_module_constants(
    functions: &mut [IrFunction],
    all_constants: &[IrConst],
) -> Vec<IrConst> {
    let mut used = HashSet::new();
    for func in &*functions {
        for block in &func.blocks {
            for inst in &block.insts {
                match inst {
                    IrInst::Const { index, .. } | IrInst::MakeStr { pool_index: index } => {
                        used.insert(*index);
                    }
                    IrInst::LoadLocal { .. }
                    | IrInst::StoreLocal { .. }
                    | IrInst::BinOp { .. }
                    | IrInst::Call { .. }
                    | IrInst::MakeFnPtr { .. }
                    | IrInst::CallIndirect { .. }
                    | IrInst::Return { .. }
                    | IrInst::Jump { .. }
                    | IrInst::JumpIf { .. }
                    | IrInst::MakeStruct { .. }
                    | IrInst::MakeEnum { .. }
                    | IrInst::GetField { .. }
                    | IrInst::SetField { .. }
                    | IrInst::MatchTag { .. }
                    | IrInst::Cast { .. }
                    | IrInst::Neg { .. }
                    | IrInst::Not { .. }
                    | IrInst::BitNot { .. }
                    | IrInst::MakeTuple { .. }
                    | IrInst::MakeArray { .. }
                    | IrInst::Index { .. }
                    | IrInst::IndexStore { .. }
                    | IrInst::PtrLoad { .. }
                    | IrInst::AddressOfLocal { .. }
                    | IrInst::LoadAggViaLocalPtr
                    | IrInst::Alloc { .. }
                    | IrInst::PtrStore { .. }
                    | IrInst::Free
                    | IrInst::Pop
                    | IrInst::StrAsSlice
                    | IrInst::TrapGivenMismatch
                    | IrInst::DropLocal { .. }
                    | IrInst::MakeSlice { .. }
                    | IrInst::MakeSliceFromPtr { .. } => {}
                }
            }
        }
    }

    let mut ordered: Vec<u32> = used.into_iter().collect();
    ordered.sort_unstable();

    let mut localized = Vec::with_capacity(ordered.len());
    let mut remap = HashMap::new();
    for old in ordered {
        let new = u32::try_from(localized.len()).unwrap_or(u32::MAX);
        remap.insert(old, new);
        if let Some(entry) = all_constants.get(old as usize) {
            localized.push(entry.clone());
        }
    }

    for func in &mut *functions {
        for block in &mut func.blocks {
            for inst in &mut block.insts {
                match inst {
                    IrInst::Const { index, .. } | IrInst::MakeStr { pool_index: index } => {
                        if let Some(&new) = remap.get(index) {
                            *index = new;
                        }
                    }
                    IrInst::LoadLocal { .. }
                    | IrInst::StoreLocal { .. }
                    | IrInst::BinOp { .. }
                    | IrInst::Call { .. }
                    | IrInst::MakeFnPtr { .. }
                    | IrInst::CallIndirect { .. }
                    | IrInst::Return { .. }
                    | IrInst::Jump { .. }
                    | IrInst::JumpIf { .. }
                    | IrInst::MakeStruct { .. }
                    | IrInst::MakeEnum { .. }
                    | IrInst::GetField { .. }
                    | IrInst::SetField { .. }
                    | IrInst::MatchTag { .. }
                    | IrInst::Cast { .. }
                    | IrInst::Neg { .. }
                    | IrInst::Not { .. }
                    | IrInst::BitNot { .. }
                    | IrInst::MakeTuple { .. }
                    | IrInst::MakeArray { .. }
                    | IrInst::Index { .. }
                    | IrInst::IndexStore { .. }
                    | IrInst::PtrLoad { .. }
                    | IrInst::AddressOfLocal { .. }
                    | IrInst::LoadAggViaLocalPtr
                    | IrInst::Alloc { .. }
                    | IrInst::PtrStore { .. }
                    | IrInst::Free
                    | IrInst::Pop
                    | IrInst::StrAsSlice
                    | IrInst::TrapGivenMismatch
                    | IrInst::DropLocal { .. }
                    | IrInst::MakeSlice { .. }
                    | IrInst::MakeSliceFromPtr { .. } => {}
                }
            }
        }
    }

    localized
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::ir::{IrBasicBlock, IrConst, IrFunctionId, IrInst};
    use crate::resolver::DefId;
    use crate::typeck::TypeId;
    use phx_bytecode::PrimitiveKind;

    #[test]
    fn localize_module_constants_keeps_only_referenced_literals() {
        let all = vec![
            IrConst::Int(1, PrimitiveKind::S32),
            IrConst::Int(2, PrimitiveKind::S32),
            IrConst::Int(3, PrimitiveKind::S32),
        ];
        let mut functions = vec![IrFunction {
            id: IrFunctionId::from_raw(0),
            def: DefId::from_raw(0),
            params: vec![],
            return_type: TypeId::from_raw(0),
            local_count: 0,
            blocks: vec![IrBasicBlock {
                insts: vec![
                    IrInst::Const {
                        index: 2,
                        ty: TypeId::from_raw(0),
                        prim_kind: PrimitiveKind::S32 as u8,
                    },
                    IrInst::Const {
                        index: 0,
                        ty: TypeId::from_raw(0),
                        prim_kind: PrimitiveKind::S32 as u8,
                    },
                ],
            }],
        }];
        let localized = localize_module_constants(&mut functions, &all);
        assert_eq!(localized.len(), 2);
        assert_eq!(
            functions[0].blocks[0].insts,
            vec![
                IrInst::Const {
                    index: 1,
                    ty: TypeId::from_raw(0),
                    prim_kind: PrimitiveKind::S32 as u8,
                },
                IrInst::Const {
                    index: 0,
                    ty: TypeId::from_raw(0),
                    prim_kind: PrimitiveKind::S32 as u8,
                },
            ]
        );
    }
}

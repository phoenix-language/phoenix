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
//! ## Pass invariants
//!
//! - **Expression cursor:** lowering walks expressions in the same pre-order as typeck
//!   (`check_expr_node`) and consumes one [`ExprId`](crate::typeck::ExprId) per expression via
//!   [`LowerCtx::expr_ty`](crate::lower::ctx::LowerCtx::expr_ty), starting at
//!   [`FunctionLayout::expr_start`](crate::typeck::FunctionLayout::expr_start) and ending at
//!   `expr_end`. [`LowerCtx::finish_expr_cursor`](crate::lower::ctx::LowerCtx::finish_expr_cursor)
//!   verifies the cursor landed on `expr_end`; drift or a missing entry in
//!   [`TypedProgram::expr_types`](crate::typeck::TypedProgram::expr_types) for an id in that range
//!   is a [`LowerError`](phx_diagnostics::LowerError).
//! - **Layout authority:** local slots, match temps, and for-in plans come from
//!   [`TypedProgram::functions`](crate::typeck::TypedProgram::functions); bytecode `type_id` and
//!   field indices come from [`ProgramLayout`](crate::typeck::ProgramLayout). Missing metadata is
//!   reported, not inferred.
//! - **CFG shape:** each function is a vector of [`IrBasicBlock`](crate::ir::IrBasicBlock)s; block 0
//!   is entry. Terminators (`Jump`, `JumpIf`, `Return`) end a block; non-terminators append to
//!   [`LowerCtx::current`](crate::lower::ctx::LowerCtx::current).
//! - **Stack-oriented IR:** expression lowering leaves one value on the implicit stack unless the
//!   expression is discarded; merge blocks for `if`/`match` rely on balanced stack depth.
//! - **Generic templates:** function and impl-method templates with empty expression ranges are
//!   skipped; monomorphized copies carry the real bodies (see [`func::lower_functions`]).
//!
//! Short-circuit `&&` and `||` lower to `JumpIf` chains (see `expr::literal::lower_short_circuit_bool`).
//!
//! ## Module map
//!
//! - [`ctx`] — shared lowering context, expression cursor, CFG builder
//! - [`func`] — top-level and impl function bodies
//! - [`expr`] — expression trees → instruction streams (`literal`, `assign`, `call`, `intrinsic`, `match`)
//! - [`stmt`] — statements, bindings, control flow
//! - [`drop_glue`] — scope-exit drops aligned with typeck binding depth

mod ctx;
mod drop_glue;
mod expr;
mod func;
mod stmt;

use crate::ir::{IrConst, IrFunction, IrInst, IrModule};
use crate::typeck::TypedProgram;
use phx_diagnostics::{LowerBag, LowerResult};
use std::collections::{HashMap, HashSet};

/// Lowers `typed` to IR for the whole program.
///
/// Walks typed AST nodes in the same expression order as typeck, uses
/// [`TypedProgram::functions`](crate::typeck::TypedProgram::functions) for [`LocalSlot`](crate::typeck::LocalSlot)
/// indices, and reads expression types from [`TypedProgram::expr_types`](crate::typeck::TypedProgram::expr_types).
///
/// On success, every lowered function satisfies [`LowerCtx::finish_expr_cursor`](crate::lower::ctx::LowerCtx::finish_expr_cursor)
/// (expression cursor at `expr_end`) and loop exit placeholders are patched to real block ids. The
/// returned [`IrModule::entry`](crate::ir::IrModule::entry) matches `typed.entry`.
///
/// # Errors
///
/// Returns [`LowerBag`] on internal invariant violations (e.g. unresolved call callee, expr cursor
/// drift, missing bytecode layout metadata). Partial IR is discarded.
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

/// Lowers only functions defined in `module_id`.
///
/// Runs a full [`lower`] pass, then filters [`IrFunction`](crate::ir::IrFunction)s whose
/// [`DefId`](crate::resolver::DefId) resolves to `module_id`. Clears [`IrModule::entry`](crate::ir::IrModule::entry)
/// when `main` lives in another module. Rewrites the constant pool to a dense subset referenced
/// by the retained functions via [`localize_module_constants`].
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
            for spanned in &block.insts {
                match &spanned.inst {
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
                    | IrInst::SliceLen
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
            for spanned in &mut block.insts {
                match &mut spanned.inst {
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
                    | IrInst::SliceLen
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
    use crate::ir::{IrBasicBlock, IrConst, IrFunctionId, IrInst, SpannedInst};
    use crate::resolver::DefId;
    use crate::typeck::TypeId;
    use phx_bytecode::PrimitiveKind;
    use phx_diagnostics::Span;

    fn span_inst(inst: IrInst) -> SpannedInst {
        SpannedInst::new(Span::new(0, 1), inst)
    }

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
                    span_inst(IrInst::Const {
                        index: 2,
                        ty: TypeId::from_raw(0),
                        prim_kind: PrimitiveKind::S32 as u8,
                    }),
                    span_inst(IrInst::Const {
                        index: 0,
                        ty: TypeId::from_raw(0),
                        prim_kind: PrimitiveKind::S32 as u8,
                    }),
                ],
            }],
        }];
        let localized = localize_module_constants(&mut functions, &all);
        assert_eq!(localized.len(), 2);
        assert_eq!(
            functions[0].blocks[0].insts,
            vec![
                span_inst(IrInst::Const {
                    index: 1,
                    ty: TypeId::from_raw(0),
                    prim_kind: PrimitiveKind::S32 as u8,
                }),
                span_inst(IrInst::Const {
                    index: 0,
                    ty: TypeId::from_raw(0),
                    prim_kind: PrimitiveKind::S32 as u8,
                }),
            ]
        );
    }
}

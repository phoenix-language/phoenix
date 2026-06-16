//! IR operand-stack simulation shared by validation and codegen.

use std::collections::{HashMap, VecDeque};

use phx_bytecode::{Opcode, StackEffectError, apply_stack_effect};
use phx_diagnostics::{IrError, Span};

use crate::ir::{IrBinOp, IrFunction, IrInst};
use crate::resolver::DefId;
use crate::typeck::{Ty, TypedProgram};

/// Stack simulation failure during codegen max-depth analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackSimError {
    /// Call or drop references a callee with no function id mapping.
    MissingCallee {
        /// Definition index of the missing callee.
        def_index: u32,
    },
}

/// Returns true when `inst` ends basic-block control flow.
#[must_use]
pub fn is_ir_terminator(inst: &IrInst) -> bool {
    matches!(
        inst,
        IrInst::Return { .. }
            | IrInst::Jump { .. }
            | IrInst::JumpIf { .. }
            | IrInst::TrapGivenMismatch
    )
}

/// Returns callee parameter count from `typed.value_types`, or `0` when unknown.
#[must_use]
pub fn fn_param_count(typed: &TypedProgram, callee: DefId) -> u32 {
    let Some(&fn_ty) = typed.value_types.get(&callee) else {
        return 0;
    };
    match typed.types.get(fn_ty) {
        Ty::Fn { params, .. } => u32::try_from(params.len()).unwrap_or(0),
        _ => 0,
    }
}

/// Applies one IR instruction's stack effect using typed callee arity.
///
/// # Errors
///
/// Returns [`IrError::StackUnderflow`] when the simulated depth would underflow.
#[allow(clippy::too_many_lines)]
pub fn apply_ir_stack_effect_typed(
    inst: &IrInst,
    depth: &mut u32,
    typed: &TypedProgram,
    def_index: u32,
    block: u32,
    inst_index: u32,
    span: Span,
) -> Result<(), IrError> {
    let map_underflow = |depth_before: u32, e: StackEffectError| match e {
        StackEffectError::Underflow => IrError::StackUnderflow {
            def_index,
            block,
            inst_index,
            depth: depth_before,
            span,
        },
        StackEffectError::MissingCallArity | StackEffectError::MissingFieldCount => {
            IrError::StackUnderflow {
                def_index,
                block,
                inst_index,
                depth: depth_before,
                span,
            }
        }
    };

    let no_call_arity = None::<u16>;
    let no_field_count = None::<u32>;
    let apply = |opcode: Opcode, depth: &mut u32| -> Result<(), IrError> {
        let depth_before = *depth;
        apply_stack_effect(opcode, depth, no_call_arity, no_field_count)
            .map_err(|e| map_underflow(depth_before, e))
    };
    match inst {
        IrInst::Const { .. } => {
            apply(Opcode::Const, depth)?;
        }
        IrInst::LoadLocal { .. } => {
            apply(Opcode::LoadLocal, depth)?;
        }
        IrInst::StoreLocal { .. } => {
            apply(Opcode::StoreLocal, depth)?;
        }
        IrInst::BinOp { op, .. } => {
            apply(ir_binop_to_opcode(*op), depth)?;
        }
        IrInst::Call { callee, .. } => {
            let depth_before = *depth;
            let arity = u16::try_from(fn_param_count(typed, *callee)).unwrap_or(0);
            apply_stack_effect(Opcode::Call, depth, Some(arity), no_field_count)
                .map_err(|e| map_underflow(depth_before, e))?;
        }
        IrInst::DropLocal { drop_fn, .. } => {
            apply(Opcode::LoadLocal, depth)?;
            let depth_before = *depth;
            let arity = u16::try_from(fn_param_count(typed, *drop_fn)).unwrap_or(1);
            apply_stack_effect(Opcode::Call, depth, Some(arity), no_field_count)
                .map_err(|e| map_underflow(depth_before, e))?;
            apply(Opcode::Pop, depth)?;
        }
        IrInst::MakeFnPtr { .. } => {
            apply(Opcode::MakeFnPtr, depth)?;
        }
        IrInst::CallIndirect { expected_arity, .. } => {
            let depth_before = *depth;
            let arity = u16::try_from(*expected_arity).unwrap_or(0);
            apply_stack_effect(Opcode::CallIndirect, depth, Some(arity), no_field_count)
                .map_err(|e| map_underflow(depth_before, e))?;
        }
        IrInst::JumpIf { .. } => {
            apply(Opcode::JumpIfTrue, depth)?;
        }
        IrInst::MakeStruct { field_count, .. } => {
            let depth_before = *depth;
            apply_stack_effect(Opcode::MakeStruct, depth, no_call_arity, Some(*field_count))
                .map_err(|e| map_underflow(depth_before, e))?;
        }
        IrInst::MakeEnum { payload_count, .. } => {
            let depth_before = *depth;
            apply_stack_effect(Opcode::MakeEnum, depth, no_call_arity, Some(*payload_count))
                .map_err(|e| map_underflow(depth_before, e))?;
        }
        IrInst::GetField { .. } => {
            apply(Opcode::GetField, depth)?;
        }
        IrInst::SetField { .. } => {
            apply(Opcode::SetField, depth)?;
        }
        IrInst::MatchTag { .. } => {
            apply(Opcode::MatchTag, depth)?;
        }
        IrInst::Cast { .. } => {
            apply(Opcode::Cast, depth)?;
        }
        IrInst::Neg { .. } => {
            apply(Opcode::Neg, depth)?;
        }
        IrInst::Not { .. } => {
            apply(Opcode::Not, depth)?;
        }
        IrInst::BitNot { .. } => {
            apply(Opcode::BitNot, depth)?;
        }
        IrInst::MakeTuple { arity } => {
            let depth_before = *depth;
            apply_stack_effect(Opcode::MakeTuple, depth, no_call_arity, Some(*arity))
                .map_err(|e| map_underflow(depth_before, e))?;
        }
        IrInst::MakeArray { len } => {
            let depth_before = *depth;
            apply_stack_effect(Opcode::MakeArray, depth, no_call_arity, Some(*len))
                .map_err(|e| map_underflow(depth_before, e))?;
        }
        IrInst::Index { .. } => {
            apply(Opcode::Index, depth)?;
        }
        IrInst::IndexStore { .. } => {
            apply(Opcode::IndexStore, depth)?;
        }
        IrInst::PtrLoad { .. } => {
            apply(Opcode::PtrLoad, depth)?;
        }
        IrInst::MakeSlice { .. } => {
            apply(Opcode::MakeSlice, depth)?;
        }
        IrInst::MakeSliceFromPtr { .. } => {
            apply(Opcode::MakeSliceFromPtr, depth)?;
        }
        IrInst::MakeStr { .. } => {
            apply(Opcode::MakeStr, depth)?;
        }
        IrInst::StrAsSlice => {
            apply(Opcode::StrAsSlice, depth)?;
        }
        IrInst::SliceLen => {
            apply(Opcode::SliceLen, depth)?;
        }
        IrInst::AddressOfLocal { .. } => {
            apply(Opcode::AddressOfLocal, depth)?;
        }
        IrInst::LoadAggViaLocalPtr => {
            apply(Opcode::LoadAggViaLocalPtr, depth)?;
        }
        IrInst::Alloc { .. } => {
            apply(Opcode::Alloc, depth)?;
        }
        IrInst::PtrStore { .. } => {
            apply(Opcode::PtrStore, depth)?;
        }
        IrInst::Free => {
            apply(Opcode::Free, depth)?;
        }
        IrInst::Pop => {
            apply(Opcode::Pop, depth)?;
        }
        IrInst::TrapGivenMismatch => {
            apply(Opcode::Trap, depth)?;
        }
        IrInst::Return { .. } | IrInst::Jump { .. } => {}
    }
    Ok(())
}

/// Applies one IR instruction's stack effect using codegen callee maps.
///
/// # Errors
///
/// Returns [`StackSimError::MissingCallee`] when a call target has no function id mapping.
#[allow(clippy::too_many_lines)]
pub fn apply_ir_stack_effect_emit(
    inst: &IrInst,
    stack: &mut u32,
    def_to_fn: &HashMap<DefId, u32>,
    fn_arity: &HashMap<u32, u16>,
) -> Result<(), StackSimError> {
    let none = None::<u32>;
    match inst {
        IrInst::Const { .. } => {
            let _ = apply_stack_effect(Opcode::Const, stack, None, none);
        }
        IrInst::LoadLocal { .. } => {
            let _ = apply_stack_effect(Opcode::LoadLocal, stack, None, none);
        }
        IrInst::StoreLocal { .. } => {
            let _ = apply_stack_effect(Opcode::StoreLocal, stack, None, none);
        }
        IrInst::BinOp { op, .. } => {
            let opcode = ir_binop_to_opcode(*op);
            let _ = apply_stack_effect(opcode, stack, None, none);
        }
        IrInst::Call { callee, .. } => {
            let fn_id = function_id_for(*callee, def_to_fn)?;
            let arity = *fn_arity.get(&fn_id).unwrap_or(&0);
            let _ = apply_stack_effect(Opcode::Call, stack, Some(arity), none);
        }
        IrInst::DropLocal { drop_fn, .. } => {
            let _ = apply_stack_effect(Opcode::LoadLocal, stack, None, none);
            let fn_id = function_id_for(*drop_fn, def_to_fn)?;
            let arity = *fn_arity.get(&fn_id).unwrap_or(&1);
            let _ = apply_stack_effect(Opcode::Call, stack, Some(arity), none);
            let _ = apply_stack_effect(Opcode::Pop, stack, None, none);
        }
        IrInst::MakeFnPtr { .. } => {
            let _ = apply_stack_effect(Opcode::MakeFnPtr, stack, None, none);
        }
        IrInst::CallIndirect { expected_arity, .. } => {
            let arity = u16::try_from(*expected_arity).unwrap_or(0);
            let _ = apply_stack_effect(Opcode::CallIndirect, stack, Some(arity), none);
        }
        IrInst::JumpIf { .. } => {
            let _ = apply_stack_effect(Opcode::JumpIfTrue, stack, None, none);
        }
        IrInst::MakeStruct { field_count, .. } => {
            let _ = apply_stack_effect(Opcode::MakeStruct, stack, None, Some(*field_count));
        }
        IrInst::MakeEnum { payload_count, .. } => {
            let _ = apply_stack_effect(Opcode::MakeEnum, stack, None, Some(*payload_count));
        }
        IrInst::GetField { .. } => {
            let _ = apply_stack_effect(Opcode::GetField, stack, None, none);
        }
        IrInst::SetField { .. } => {
            let _ = apply_stack_effect(Opcode::SetField, stack, None, none);
        }
        IrInst::MatchTag { .. } => {
            let _ = apply_stack_effect(Opcode::MatchTag, stack, None, none);
        }
        IrInst::Cast { .. } => {
            let _ = apply_stack_effect(Opcode::Cast, stack, None, none);
        }
        IrInst::Neg { .. } => {
            let _ = apply_stack_effect(Opcode::Neg, stack, None, none);
        }
        IrInst::Not { .. } => {
            let _ = apply_stack_effect(Opcode::Not, stack, None, none);
        }
        IrInst::BitNot { .. } => {
            let _ = apply_stack_effect(Opcode::BitNot, stack, None, none);
        }
        IrInst::MakeTuple { arity } => {
            let _ = apply_stack_effect(Opcode::MakeTuple, stack, None, Some(*arity));
        }
        IrInst::MakeArray { len } => {
            let _ = apply_stack_effect(Opcode::MakeArray, stack, None, Some(*len));
        }
        IrInst::Index { .. } => {
            let _ = apply_stack_effect(Opcode::Index, stack, None, none);
        }
        IrInst::IndexStore { .. } => {
            let _ = apply_stack_effect(Opcode::IndexStore, stack, None, none);
        }
        IrInst::PtrLoad { .. } => {
            let _ = apply_stack_effect(Opcode::PtrLoad, stack, None, none);
        }
        IrInst::MakeSlice { .. } => {
            let _ = apply_stack_effect(Opcode::MakeSlice, stack, None, none);
        }
        IrInst::MakeSliceFromPtr { .. } => {
            let _ = apply_stack_effect(Opcode::MakeSliceFromPtr, stack, None, none);
        }
        IrInst::MakeStr { .. } => {
            let _ = apply_stack_effect(Opcode::MakeStr, stack, None, none);
        }
        IrInst::StrAsSlice => {
            let _ = apply_stack_effect(Opcode::StrAsSlice, stack, None, none);
        }
        IrInst::SliceLen => {
            let _ = apply_stack_effect(Opcode::SliceLen, stack, None, none);
        }
        IrInst::AddressOfLocal { .. } => {
            let _ = apply_stack_effect(Opcode::AddressOfLocal, stack, None, none);
        }
        IrInst::LoadAggViaLocalPtr => {
            let _ = apply_stack_effect(Opcode::LoadAggViaLocalPtr, stack, None, none);
        }
        IrInst::Alloc { .. } => {
            let _ = apply_stack_effect(Opcode::Alloc, stack, None, none);
        }
        IrInst::PtrStore { .. } => {
            let _ = apply_stack_effect(Opcode::PtrStore, stack, None, none);
        }
        IrInst::Free => {
            let _ = apply_stack_effect(Opcode::Free, stack, None, none);
        }
        IrInst::Pop => {
            let _ = apply_stack_effect(Opcode::Pop, stack, None, none);
        }
        IrInst::TrapGivenMismatch => {
            let _ = apply_stack_effect(Opcode::Trap, stack, None, none);
        }
        IrInst::Return { .. } | IrInst::Jump { .. } => {}
    }
    Ok(())
}

/// CFG-aware stack validation: merge blocks must agree on entry depth.
///
/// # Errors
///
/// Returns the first [`IrError`] encountered during simulation.
pub fn analyze_ir_stack_cfg(func: &IrFunction, typed: &TypedProgram) -> Result<(), IrError> {
    if func.blocks.is_empty() {
        return Ok(());
    }

    let def_index = func.def.index();
    let mut entry_depth: HashMap<u32, u32> = HashMap::new();
    let mut worklist = VecDeque::from([0u32]);
    entry_depth.insert(0, 0);
    let visit_limit = u32::try_from(func.blocks.len())
        .unwrap_or(u32::MAX)
        .saturating_mul(64)
        .max(64);
    let mut visits = 0u32;

    while let Some(block_id) = worklist.pop_front() {
        visits = visits.saturating_add(1);
        if visits > visit_limit {
            break;
        }
        let Some(block) = func.blocks.get(block_id as usize) else {
            continue;
        };
        let mut depth = *entry_depth.get(&block_id).unwrap_or(&0);

        let mut block_terminates = false;
        for (inst_index, spanned) in block.insts.iter().enumerate() {
            apply_ir_stack_effect_typed(
                &spanned.inst,
                &mut depth,
                typed,
                def_index,
                block_id,
                u32::try_from(inst_index).unwrap_or(u32::MAX),
                spanned.span,
            )?;

            match &spanned.inst {
                IrInst::Jump { target } => {
                    enqueue_ir_edge_strict(
                        def_index,
                        block_id,
                        *target,
                        depth,
                        spanned.span,
                        &mut entry_depth,
                        &mut worklist,
                    )?;
                    block_terminates = true;
                }
                IrInst::JumpIf {
                    then_block,
                    else_block,
                } => {
                    enqueue_ir_edge_strict(
                        def_index,
                        block_id,
                        *then_block,
                        depth,
                        spanned.span,
                        &mut entry_depth,
                        &mut worklist,
                    )?;
                    enqueue_ir_edge_strict(
                        def_index,
                        block_id,
                        *else_block,
                        depth,
                        spanned.span,
                        &mut entry_depth,
                        &mut worklist,
                    )?;
                    block_terminates = true;
                }
                IrInst::Return { .. } | IrInst::TrapGivenMismatch => {
                    block_terminates = true;
                }
                _ => {}
            }
        }

        if block.insts.is_empty() || !block_terminates {
            let next = block_id.saturating_add(1);
            if (next as usize) < func.blocks.len() {
                let fallthrough_span = block
                    .insts
                    .last()
                    .map_or(Span::new(0, 1), |spanned| spanned.span);
                enqueue_ir_edge_strict(
                    def_index,
                    block_id,
                    next,
                    depth,
                    fallthrough_span,
                    &mut entry_depth,
                    &mut worklist,
                )?;
            }
        }
    }

    Ok(())
}

/// CFG-aware max stack depth for codegen (conservative merge at join blocks).
///
/// # Errors
///
/// Returns [`StackSimError::MissingCallee`] when a call target has no function id mapping.
pub fn compute_ir_stack_max(
    func: &IrFunction,
    def_to_fn: &HashMap<DefId, u32>,
    fn_arity: &HashMap<u32, u16>,
) -> Result<u32, StackSimError> {
    if func.blocks.is_empty() {
        return Ok(0);
    }

    let mut entry_depth: HashMap<u32, u32> = HashMap::new();
    let mut max_stack = 0u32;
    let mut worklist = VecDeque::from([0u32]);
    entry_depth.insert(0, 0);
    let visit_limit = u32::try_from(func.blocks.len())
        .unwrap_or(u32::MAX)
        .saturating_mul(64)
        .max(64);
    let mut visits = 0u32;

    while let Some(block_id) = worklist.pop_front() {
        visits = visits.saturating_add(1);
        if visits > visit_limit {
            break;
        }
        let Some(block) = func.blocks.get(block_id as usize) else {
            continue;
        };
        let mut depth = *entry_depth.get(&block_id).unwrap_or(&0);
        max_stack = max_stack.max(depth);

        let mut block_terminates = false;
        for spanned in &block.insts {
            apply_ir_stack_effect_emit(&spanned.inst, &mut depth, def_to_fn, fn_arity)?;
            max_stack = max_stack.max(depth);

            match &spanned.inst {
                IrInst::Jump { target } => {
                    enqueue_ir_edge_max(
                        block_id,
                        *target,
                        depth,
                        &mut entry_depth,
                        &mut worklist,
                        &mut max_stack,
                    );
                    block_terminates = true;
                }
                IrInst::JumpIf {
                    then_block,
                    else_block,
                } => {
                    enqueue_ir_edge_max(
                        block_id,
                        *then_block,
                        depth,
                        &mut entry_depth,
                        &mut worklist,
                        &mut max_stack,
                    );
                    enqueue_ir_edge_max(
                        block_id,
                        *else_block,
                        depth,
                        &mut entry_depth,
                        &mut worklist,
                        &mut max_stack,
                    );
                    block_terminates = true;
                }
                IrInst::Return { .. } | IrInst::TrapGivenMismatch => {
                    block_terminates = true;
                }
                _ => {}
            }
        }

        if block.insts.is_empty() || !block_terminates {
            let next = block_id.saturating_add(1);
            if (next as usize) < func.blocks.len() {
                enqueue_ir_edge_max(
                    block_id,
                    next,
                    depth,
                    &mut entry_depth,
                    &mut worklist,
                    &mut max_stack,
                );
            }
        }
    }

    let conservative_floor = u32::try_from(func.params.len())
        .unwrap_or(0)
        .saturating_add(func.local_count)
        .saturating_add(16);
    Ok(max_stack.max(conservative_floor))
}

fn function_id_for(def: DefId, def_to_fn: &HashMap<DefId, u32>) -> Result<u32, StackSimError> {
    def_to_fn
        .get(&def)
        .copied()
        .ok_or(StackSimError::MissingCallee {
            def_index: def.index(),
        })
}

fn ir_binop_to_opcode(op: IrBinOp) -> Opcode {
    match op {
        IrBinOp::Add => Opcode::Add,
        IrBinOp::Sub => Opcode::Sub,
        IrBinOp::Mul => Opcode::Mul,
        IrBinOp::Div => Opcode::Div,
        IrBinOp::Eq => Opcode::Eq,
        IrBinOp::Lt => Opcode::Lt,
        IrBinOp::Ne => Opcode::Ne,
        IrBinOp::Le => Opcode::Le,
        IrBinOp::Ge => Opcode::Ge,
        IrBinOp::Mod => Opcode::Mod,
        IrBinOp::Pow => Opcode::Pow,
        IrBinOp::BitAnd => Opcode::BitAnd,
        IrBinOp::BitOr => Opcode::BitOr,
        IrBinOp::BitXor => Opcode::BitXor,
        IrBinOp::Shl => Opcode::Shl,
        IrBinOp::Shr => Opcode::Shr,
    }
}

fn enqueue_ir_edge_strict(
    def_index: u32,
    from: u32,
    target: u32,
    depth: u32,
    span: Span,
    entry_depth: &mut HashMap<u32, u32>,
    worklist: &mut VecDeque<u32>,
) -> Result<(), IrError> {
    if target <= from {
        match entry_depth.get(&target).copied() {
            None => {
                entry_depth.insert(target, depth);
            }
            Some(existing) if existing == depth => {}
            Some(existing) => {
                return Err(IrError::JoinDepthMismatch {
                    def_index,
                    block: target,
                    expected: existing,
                    found: depth,
                    span,
                });
            }
        }
        return Ok(());
    }
    try_enqueue_ir_block_strict(def_index, target, depth, span, entry_depth, worklist)
}

fn try_enqueue_ir_block_strict(
    def_index: u32,
    target: u32,
    depth: u32,
    span: Span,
    entry_depth: &mut HashMap<u32, u32>,
    worklist: &mut VecDeque<u32>,
) -> Result<(), IrError> {
    match entry_depth.get(&target).copied() {
        None => {
            entry_depth.insert(target, depth);
            worklist.push_back(target);
        }
        Some(existing) if existing == depth => {}
        Some(existing) => {
            return Err(IrError::JoinDepthMismatch {
                def_index,
                block: target,
                expected: existing,
                found: depth,
                span,
            });
        }
    }
    Ok(())
}

fn enqueue_ir_edge_max(
    from: u32,
    target: u32,
    depth: u32,
    entry_depth: &mut HashMap<u32, u32>,
    worklist: &mut VecDeque<u32>,
    max_stack: &mut u32,
) {
    *max_stack = (*max_stack).max(depth);
    if target <= from {
        let merged = entry_depth.get(&target).copied().unwrap_or(0).max(depth);
        entry_depth.insert(target, merged);
        return;
    }
    try_enqueue_ir_block_max(target, depth, entry_depth, worklist);
}

fn try_enqueue_ir_block_max(
    target: u32,
    depth: u32,
    entry_depth: &mut HashMap<u32, u32>,
    worklist: &mut VecDeque<u32>,
) {
    match entry_depth.get(&target).copied() {
        None => {
            entry_depth.insert(target, depth);
            worklist.push_back(target);
        }
        Some(existing) if existing == depth => {}
        Some(existing) => {
            let merged = existing.max(depth);
            if merged != existing {
                entry_depth.insert(target, merged);
                worklist.push_back(target);
            }
        }
    }
}

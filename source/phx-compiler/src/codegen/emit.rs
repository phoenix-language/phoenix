//! Emit IR CFG to flat PHX0 instruction bytes.
//!
//! ## Stack convention
//!
//! Operands are evaluated left-to-right (bottom = left, top = right). Binary ops pop `b`,
//! then `a`, and push `op(a, b)`. Call leaves `arity` arguments on the stack (bottom = first param).

use std::collections::HashMap;

use phx_bytecode::{Instruction, Opcode, apply_stack_effect};

use crate::ir::{IrBinOp, IrFunction, IrInst};
use crate::resolver::DefId;

use super::const_pool::ConstPoolBuilder;
use super::error::CodegenError;

/// Result of emitting one function body.
#[derive(Debug)]
pub struct EmittedFunction {
    /// Encoded instructions for this function.
    pub code: Vec<u8>,
    /// Maximum operand stack depth observed during emission.
    pub stack_max: u16,
}

/// Emits one function's CFG to bytecode bytes.
///
/// # Errors
///
/// Returns [`CodegenError`] when a constant pool lookup fails, a callee or drop function
/// has no function id mapping, a jump target block has no code offset, or a type id is
/// missing from the module-local remap.
pub fn emit_function(
    func: &IrFunction,
    pool: &mut ConstPoolBuilder,
    def_to_fn: &HashMap<DefId, u32>,
    fn_arity: &HashMap<u32, u16>,
    type_remap: Option<&HashMap<u32, u32>>,
) -> Result<EmittedFunction, CodegenError> {
    let block_starts = compute_block_starts(func, pool, def_to_fn, type_remap)?;
    let (code, stack_max) =
        emit_blocks(func, pool, def_to_fn, fn_arity, &block_starts, type_remap)?;
    Ok(EmittedFunction { code, stack_max })
}

fn map_type_id(global: u32, type_remap: Option<&HashMap<u32, u32>>) -> Result<u32, CodegenError> {
    match type_remap {
        None => Ok(global),
        Some(map) => map
            .get(&global)
            .copied()
            .ok_or(CodegenError::MissingTypeId { type_id: global }),
    }
}

fn function_id_for(def: DefId, def_to_fn: &HashMap<DefId, u32>) -> Result<u32, CodegenError> {
    def_to_fn
        .get(&def)
        .copied()
        .ok_or(CodegenError::MissingCallee {
            def_index: def.index(),
        })
}

fn block_offset(block: u32, starts: &[u32]) -> Result<u32, CodegenError> {
    starts
        .get(block as usize)
        .copied()
        .ok_or(CodegenError::InvalidJumpBlock { block })
}

fn compute_block_starts(
    func: &IrFunction,
    pool: &ConstPoolBuilder,
    def_to_fn: &HashMap<DefId, u32>,
    type_remap: Option<&HashMap<u32, u32>>,
) -> Result<Vec<u32>, CodegenError> {
    let n = func.blocks.len();
    let mut starts = vec![0u32; n];
    let mut scratch = Vec::new();
    let mut offset = 0u32;
    for (block_id, block) in func.blocks.iter().enumerate() {
        starts[block_id] = offset;
        for inst in &block.insts {
            scratch.clear();
            emit_inst(&mut scratch, inst, pool, def_to_fn, &starts, type_remap)?;
            offset = offset.saturating_add(u32::try_from(scratch.len()).unwrap_or(u32::MAX));
        }
    }
    Ok(starts)
}

#[allow(clippy::too_many_lines)]
fn apply_ir_stack_effect(
    inst: &IrInst,
    stack: &mut u32,
    def_to_fn: &HashMap<DefId, u32>,
    fn_arity: &HashMap<u32, u16>,
) -> Result<(), CodegenError> {
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

fn emit_blocks(
    func: &IrFunction,
    pool: &mut ConstPoolBuilder,
    def_to_fn: &HashMap<DefId, u32>,
    fn_arity: &HashMap<u32, u16>,
    block_starts: &[u32],
    type_remap: Option<&HashMap<u32, u32>>,
) -> Result<(Vec<u8>, u16), CodegenError> {
    let mut out = Vec::new();
    for block in &func.blocks {
        for inst in &block.insts {
            emit_inst(&mut out, inst, pool, def_to_fn, block_starts, type_remap)?;
        }
    }
    let max_stack = compute_ir_stack_max(func, def_to_fn, fn_arity)?;
    let stack_max = u16::try_from(max_stack).unwrap_or(u16::MAX);
    Ok((out, stack_max))
}

/// CFG-aware max stack depth for IR (short-circuit paths are not linear in block order).
fn compute_ir_stack_max(
    func: &IrFunction,
    def_to_fn: &HashMap<DefId, u32>,
    fn_arity: &HashMap<u32, u16>,
) -> Result<u32, CodegenError> {
    use std::collections::VecDeque;

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
        for inst in &block.insts {
            apply_ir_stack_effect(inst, &mut depth, def_to_fn, fn_arity)?;
            max_stack = max_stack.max(depth);

            match inst {
                IrInst::Jump { target } => {
                    enqueue_ir_edge(
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
                    enqueue_ir_edge(
                        block_id,
                        *then_block,
                        depth,
                        &mut entry_depth,
                        &mut worklist,
                        &mut max_stack,
                    );
                    enqueue_ir_edge(
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

        // Empty merge blocks share the next block's bytecode offset; still enqueue fallthrough.
        if block.insts.is_empty() || !block_terminates {
            let next = block_id.saturating_add(1);
            if (next as usize) < func.blocks.len() {
                enqueue_ir_edge(
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

/// Enqueues a CFG edge for stack-depth fixpoint. Back-edges merge depth without re-walking
/// the header (prevents infinite re-simulation on `while` loops).
fn enqueue_ir_edge(
    from: u32,
    target: u32,
    depth: u32,
    entry_depth: &mut std::collections::HashMap<u32, u32>,
    worklist: &mut std::collections::VecDeque<u32>,
    max_stack: &mut u32,
) {
    *max_stack = (*max_stack).max(depth);
    if target <= from {
        let merged = entry_depth.get(&target).copied().unwrap_or(0).max(depth);
        entry_depth.insert(target, merged);
        return;
    }
    try_enqueue_ir_block(target, depth, entry_depth, worklist);
}

fn try_enqueue_ir_block(
    target: u32,
    depth: u32,
    entry_depth: &mut std::collections::HashMap<u32, u32>,
    worklist: &mut std::collections::VecDeque<u32>,
) {
    match entry_depth.get(&target).copied() {
        None => {
            entry_depth.insert(target, depth);
            worklist.push_back(target);
        }
        Some(existing) if existing == depth => {}
        Some(existing) => {
            // Conservative stack allocation: merge with max depth and re-walk if increased.
            let merged = existing.max(depth);
            if merged != existing {
                entry_depth.insert(target, merged);
                worklist.push_back(target);
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn emit_inst(
    out: &mut Vec<u8>,
    inst: &IrInst,
    pool: &ConstPoolBuilder,
    def_to_fn: &HashMap<DefId, u32>,
    block_starts: &[u32],
    type_remap: Option<&HashMap<u32, u32>>,
) -> Result<(), CodegenError> {
    match inst {
        IrInst::Const {
            index, prim_kind, ..
        } => {
            let pool_idx = pool.pool_index_for_literal(*index)?;
            out.extend(encode(Opcode::Const, &[pool_idx, u32::from(*prim_kind)]));
        }
        IrInst::LoadLocal {
            slot, prim_kind, ..
        } => {
            out.extend(encode(
                Opcode::LoadLocal,
                &[slot.index(), u32::from(*prim_kind)],
            ));
        }
        IrInst::StoreLocal {
            slot, prim_kind, ..
        } => {
            out.extend(encode(
                Opcode::StoreLocal,
                &[slot.index(), u32::from(*prim_kind)],
            ));
        }
        IrInst::BinOp { op, prim_kind, .. } => {
            out.extend(encode(ir_binop_to_opcode(*op), &[u32::from(*prim_kind)]));
        }
        IrInst::Call { callee, .. } => {
            let fn_id = function_id_for(*callee, def_to_fn)?;
            out.extend(encode(Opcode::Call, &[fn_id]));
        }
        IrInst::MakeFnPtr {
            target_kind,
            target_id,
            ..
        } => {
            out.extend(encode(Opcode::MakeFnPtr, &[*target_kind, *target_id]));
        }
        IrInst::CallIndirect {
            sig_type_id,
            expected_arity,
            ..
        } => {
            let sig = map_type_id(*sig_type_id, type_remap)?;
            out.extend(encode(Opcode::CallIndirect, &[*expected_arity, sig]));
        }
        IrInst::Return { .. } => {
            out.extend(encode(Opcode::Return, &[]));
        }
        IrInst::Jump { target } => {
            let off = block_offset(*target, block_starts)?;
            out.extend(encode(Opcode::Jump, &[off]));
        }
        IrInst::JumpIf {
            then_block,
            else_block,
        } => {
            let then_off = block_offset(*then_block, block_starts)?;
            let else_off = block_offset(*else_block, block_starts)?;
            out.extend(encode(Opcode::JumpIfTrue, &[then_off]));
            out.extend(encode(Opcode::Jump, &[else_off]));
        }
        IrInst::MakeStruct {
            type_id,
            field_count,
        } => {
            let ty = map_type_id(*type_id, type_remap)?;
            out.extend(encode(Opcode::MakeStruct, &[ty, *field_count]));
        }
        IrInst::MakeEnum {
            type_id,
            variant_tag,
            payload_count,
        } => {
            let ty = map_type_id(*type_id, type_remap)?;
            out.extend(encode(
                Opcode::MakeEnum,
                &[ty, *variant_tag, *payload_count],
            ));
        }
        IrInst::GetField {
            type_id,
            field_index,
            ..
        } => {
            let ty = map_type_id(*type_id, type_remap)?;
            out.extend(encode(Opcode::GetField, &[ty, *field_index]));
        }
        IrInst::SetField {
            type_id,
            field_index,
        } => {
            let ty = map_type_id(*type_id, type_remap)?;
            out.extend(encode(Opcode::SetField, &[ty, *field_index]));
        }
        IrInst::MatchTag {
            type_id,
            variant_tag,
        } => {
            let ty = map_type_id(*type_id, type_remap)?;
            out.extend(encode(Opcode::MatchTag, &[ty, *variant_tag]));
        }
        IrInst::Cast { from_kind, to_kind } => {
            out.extend(encode(
                Opcode::Cast,
                &[u32::from(*from_kind), u32::from(*to_kind)],
            ));
        }
        IrInst::Neg { prim_kind, .. } => {
            out.extend(encode(Opcode::Neg, &[u32::from(*prim_kind)]));
        }
        IrInst::Not { prim_kind, .. } => {
            out.extend(encode(Opcode::Not, &[u32::from(*prim_kind)]));
        }
        IrInst::BitNot { prim_kind, .. } => {
            out.extend(encode(Opcode::BitNot, &[u32::from(*prim_kind)]));
        }
        IrInst::MakeTuple { arity } => {
            out.extend(encode(Opcode::MakeTuple, &[*arity]));
        }
        IrInst::MakeArray { len } => {
            out.extend(encode(Opcode::MakeArray, &[*len]));
        }
        IrInst::Index { .. } => {
            out.extend(encode(Opcode::Index, &[]));
        }
        IrInst::IndexStore {
            prim_kind, signed, ..
        } => {
            out.extend(encode(
                Opcode::IndexStore,
                &[u32::from(*prim_kind), u32::from(*signed)],
            ));
        }
        IrInst::PtrLoad {
            prim_kind, signed, ..
        } => {
            out.extend(encode(
                Opcode::PtrLoad,
                &[u32::from(*prim_kind), u32::from(*signed)],
            ));
        }
        IrInst::AddressOfLocal { slot } => {
            out.extend(encode(Opcode::AddressOfLocal, &[slot.index()]));
        }
        IrInst::LoadAggViaLocalPtr => {
            out.extend(encode(Opcode::LoadAggViaLocalPtr, &[]));
        }
        IrInst::Alloc { .. } => {
            out.extend(encode(Opcode::Alloc, &[]));
        }
        IrInst::PtrStore {
            prim_kind, signed, ..
        } => {
            out.extend(encode(
                Opcode::PtrStore,
                &[u32::from(*prim_kind), u32::from(*signed)],
            ));
        }
        IrInst::Free => {
            out.extend(encode(Opcode::Free, &[]));
        }
        IrInst::Pop => {
            out.extend(encode(Opcode::Pop, &[]));
        }
        IrInst::MakeSlice { elem_kind } => {
            out.extend(encode(Opcode::MakeSlice, &[u32::from(*elem_kind)]));
        }
        IrInst::MakeSliceFromPtr { elem_kind } => {
            out.extend(encode(Opcode::MakeSliceFromPtr, &[u32::from(*elem_kind)]));
        }
        IrInst::MakeStr { pool_index } => {
            let pool_idx = pool.pool_index_for_literal(*pool_index)?;
            out.extend(encode(Opcode::MakeStr, &[pool_idx]));
        }
        IrInst::StrAsSlice => {
            out.extend(encode(Opcode::StrAsSlice, &[]));
        }
        IrInst::TrapGivenMismatch => {
            out.extend(encode(Opcode::Trap, &[]));
        }
        IrInst::DropLocal {
            slot,
            prim_kind,
            drop_fn,
            ..
        } => {
            out.extend(encode(
                Opcode::LoadLocal,
                &[slot.index(), u32::from(*prim_kind)],
            ));
            let fn_id = function_id_for(*drop_fn, def_to_fn)?;
            out.extend(encode(Opcode::Call, &[fn_id]));
        }
    }
    Ok(())
}

fn encode(opcode: Opcode, operands: &[u32]) -> Vec<u8> {
    Instruction {
        opcode,
        operands: operands.to_vec(),
    }
    .encode()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrBasicBlock, IrFunction, IrFunctionId, IrInst, LocalSlot};
    use crate::typeck::TypeId;

    fn minimal_func(insts: Vec<IrInst>) -> IrFunction {
        IrFunction {
            id: IrFunctionId::from_raw(0),
            def: DefId::from_raw(0),
            params: Vec::new(),
            return_type: TypeId::from_raw(0),
            local_count: 0,
            blocks: vec![IrBasicBlock { insts }],
        }
    }

    #[test]
    fn emit_fails_on_invalid_jump_block() {
        let func = minimal_func(vec![IrInst::Jump { target: 99 }]);
        let mut pool = ConstPoolBuilder::new();
        let def_to_fn = HashMap::new();
        let fn_arity = HashMap::new();
        match emit_function(&func, &mut pool, &def_to_fn, &fn_arity, None) {
            Err(CodegenError::InvalidJumpBlock { block: 99 }) => {}
            other => panic!("expected InvalidJumpBlock {{ block: 99 }}, got {other:?}"),
        }
    }

    #[test]
    fn emit_fails_on_missing_callee() {
        let func = minimal_func(vec![IrInst::Call {
            callee: DefId::from_raw(42),
            ret: TypeId::from_raw(0),
        }]);
        let mut pool = ConstPoolBuilder::new();
        let def_to_fn = HashMap::new();
        let fn_arity = HashMap::new();
        match emit_function(&func, &mut pool, &def_to_fn, &fn_arity, None) {
            Err(CodegenError::MissingCallee { def_index: 42 }) => {}
            other => panic!("expected MissingCallee {{ def_index: 42 }}, got {other:?}"),
        }
    }

    #[test]
    fn emit_fails_on_missing_drop_fn() {
        let func = minimal_func(vec![IrInst::DropLocal {
            slot: LocalSlot::from_raw(0),
            ty: TypeId::from_raw(0),
            drop_fn: DefId::from_raw(7),
            prim_kind: 0,
        }]);
        let mut pool = ConstPoolBuilder::new();
        let def_to_fn = HashMap::new();
        let fn_arity = HashMap::new();
        match emit_function(&func, &mut pool, &def_to_fn, &fn_arity, None) {
            Err(CodegenError::MissingCallee { def_index: 7 }) => {}
            other => panic!("expected MissingCallee {{ def_index: 7 }}, got {other:?}"),
        }
    }
}

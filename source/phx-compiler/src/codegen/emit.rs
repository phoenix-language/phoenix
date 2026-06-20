//! IR control-flow graph → flat PHX0 instruction bytes.
//!
//! Part of the [`codegen`](crate::codegen) pass. [`emit_function`] walks one
//! [`IrFunction`](crate::ir::IrFunction)'s basic blocks in order, maps each
//! [`IrInst`](crate::ir::IrInst) to wire-format [`Instruction`](phx_bytecode::Instruction)
//! bytes, and records per-PC source spans for debug builds.
//!
//! ## Two-phase layout
//!
//! Forward jumps need block start offsets before emission finishes. Emission therefore runs
//! in two passes: [`compute_block_starts`] dry-runs instruction encoding to build a
//! block-id → code-offset table, then [`emit_blocks`] writes the final bytes using those
//! offsets for [`IrInst::Jump`](crate::ir::IrInst::Jump) and
//! [`IrInst::JumpIf`](crate::ir::IrInst::JumpIf) targets.
//!
//! ## Entry point
//!
//! - [`emit_function`] — called from [`codegen`](crate::codegen::codegen) and
//!   [`codegen_module`](crate::codegen::codegen_module) for each function body
//!
//! ## Stack convention
//!
//! Operand evaluation matches the VM stack model: left-to-right (bottom = left, top = right).
//! Binary ops pop `b`, then `a`, and push `op(a, b)`. [`IrInst::Call`](crate::ir::IrInst::Call)
//! leaves `arity` arguments on the stack (bottom = first param). After emission,
//! [`compute_ir_stack_max`](crate::ir::compute_ir_stack_max) validates the observed
//! [`EmittedFunction::stack_max`].

use std::collections::HashMap;

use phx_bytecode::{InstrError, Instruction, Opcode};
use phx_diagnostics::Span;

use crate::ir::{IrBinOp, IrFunction, IrInst};
use crate::ir::{StackSimError, compute_ir_stack_max};
use crate::resolver::{DefId, DefKind, ResolvedProgram};

use super::const_pool::ConstPoolBuilder;
use super::error::CodegenError;

/// Encoded body for one [`IrFunction`](crate::ir::IrFunction).
///
/// Produced by [`emit_function`] and stitched into the module code section by
/// [`codegen`](crate::codegen::codegen) / [`codegen_module`](crate::codegen::codegen_module).
#[derive(Debug)]
pub struct EmittedFunction {
    /// Contiguous PHX0 instruction bytes for this function (function-local PCs).
    pub code: Vec<u8>,
    /// Maximum operand stack depth required at run time, from [`compute_ir_stack_max`](crate::ir::compute_ir_stack_max).
    pub stack_max: u16,
    /// `(pc, span)` pairs at each instruction start, for debug PC→source mapping.
    pub pc_spans: Vec<(u32, Span)>,
}

/// Emits one function's CFG to bytecode bytes.
///
/// Flattens `func.blocks` in block-id order, resolves [`DefId`](crate::resolver::DefId)
/// callees through `def_to_fn`, and optionally remaps aggregate / indirect-call type ids
/// via `type_remap` (project builds only; `None` for single-unit [`codegen`](crate::codegen::codegen)).
///
/// # Errors
///
/// Returns [`CodegenError`] when a constant pool lookup fails, a callee or drop function
/// has no function id mapping, a jump target block has no code offset, or a type id is
/// missing from the module-local remap.
///
/// # Panics
///
/// Never panics on malformed user input.
pub fn emit_function(
    func: &IrFunction,
    pool: &mut ConstPoolBuilder,
    def_to_fn: &HashMap<DefId, u32>,
    fn_arity: &HashMap<u32, u16>,
    type_remap: Option<&HashMap<u32, u32>>,
    resolved: &ResolvedProgram,
) -> Result<EmittedFunction, CodegenError> {
    let block_starts = compute_block_starts(func, pool, def_to_fn, type_remap, resolved)?;
    let (code, stack_max, pc_spans) = emit_blocks(
        func,
        pool,
        def_to_fn,
        fn_arity,
        &block_starts,
        type_remap,
        resolved,
    )?;
    Ok(EmittedFunction {
        code,
        stack_max,
        pc_spans,
    })
}

/// Maps a layout-global type id to the module-local id when `type_remap` is present.
fn map_type_id(global: u32, type_remap: Option<&HashMap<u32, u32>>) -> Result<u32, CodegenError> {
    match type_remap {
        None => Ok(global),
        Some(map) => map
            .get(&global)
            .copied()
            .ok_or(CodegenError::MissingTypeId { type_id: global }),
    }
}

/// Resolves a [`DefId`](crate::resolver::DefId) to its PHX0 function id for direct calls.
fn function_id_for(def: DefId, def_to_fn: &HashMap<DefId, u32>) -> Result<u32, CodegenError> {
    def_to_fn
        .get(&def)
        .copied()
        .ok_or(CodegenError::MissingCallee {
            def_index: def.index(),
        })
}

/// Looks up the code-section byte offset for basic block `block`.
fn block_offset(block: u32, starts: &[u32]) -> Result<u32, CodegenError> {
    starts
        .get(block as usize)
        .copied()
        .ok_or(CodegenError::InvalidJumpBlock { block })
}

/// Computes the foreign-stub index for an [`DefKind::ExternFn`](crate::resolver::DefKind::ExternFn) def.
///
/// Foreign targets use a dense id space separate from module function ids; this counts prior
/// extern declarations in definition order.
fn foreign_stub_id(def: DefId, resolved: &ResolvedProgram) -> Result<u32, CodegenError> {
    let record = resolved
        .defs
        .get(def.index() as usize)
        .ok_or(CodegenError::MissingCallee {
            def_index: def.index(),
        })?;
    if record.kind != DefKind::ExternFn {
        return Err(CodegenError::MissingCallee {
            def_index: def.index(),
        });
    }
    let id = resolved.defs[..def.index() as usize]
        .iter()
        .filter(|d| d.kind == DefKind::ExternFn)
        .count();
    u32::try_from(id).map_err(|_| CodegenError::MissingCallee {
        def_index: def.index(),
    })
}

/// Dry-runs instruction encoding to compute each basic block's start offset.
///
/// Used by [`emit_function`] before the final [`emit_blocks`] pass so forward jumps can
/// reference absolute code offsets.
fn compute_block_starts(
    func: &IrFunction,
    pool: &ConstPoolBuilder,
    def_to_fn: &HashMap<DefId, u32>,
    type_remap: Option<&HashMap<u32, u32>>,
    resolved: &ResolvedProgram,
) -> Result<Vec<u32>, CodegenError> {
    let n = func.blocks.len();
    let mut starts = vec![0u32; n];
    let mut scratch = Vec::new();
    let mut offset = 0u32;
    for (block_id, block) in func.blocks.iter().enumerate() {
        starts[block_id] = offset;
        for spanned in &block.insts {
            scratch.clear();
            emit_inst(
                &mut scratch,
                &spanned.inst,
                pool,
                def_to_fn,
                &starts,
                type_remap,
                resolved,
            )?;
            offset = offset.saturating_add(u32::try_from(scratch.len()).unwrap_or(u32::MAX));
        }
    }
    Ok(starts)
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

/// Encodes all blocks in `func`, recording PC spans and computing `stack_max`.
#[allow(clippy::type_complexity)]
fn emit_blocks(
    func: &IrFunction,
    pool: &mut ConstPoolBuilder,
    def_to_fn: &HashMap<DefId, u32>,
    fn_arity: &HashMap<u32, u16>,
    block_starts: &[u32],
    type_remap: Option<&HashMap<u32, u32>>,
    resolved: &ResolvedProgram,
) -> Result<(Vec<u8>, u16, Vec<(u32, Span)>), CodegenError> {
    let mut out = Vec::new();
    let mut pc_spans = Vec::new();
    for block in &func.blocks {
        for spanned in &block.insts {
            let base_pc = u32::try_from(out.len()).unwrap_or(u32::MAX);
            let inst_start = out.len();
            emit_inst(
                &mut out,
                &spanned.inst,
                pool,
                def_to_fn,
                block_starts,
                type_remap,
                resolved,
            )?;
            record_instruction_pc_spans(&mut pc_spans, base_pc, &out[inst_start..], spanned.span);
        }
    }
    let max_stack = compute_ir_stack_max(func, def_to_fn, fn_arity).map_err(map_stack_sim_error)?;
    let stack_max = u16::try_from(max_stack).unwrap_or(u16::MAX);
    Ok((out, stack_max, pc_spans))
}

fn map_stack_sim_error(err: StackSimError) -> CodegenError {
    match err {
        StackSimError::MissingCallee { def_index } => CodegenError::MissingCallee { def_index },
    }
}

/// Records one `(pc, span)` row at the start of each encoded instruction in `code`.
///
/// IR instructions such as [`IrInst::JumpIf`](crate::ir::IrInst::JumpIf),
/// [`IrInst::DropLocal`](crate::ir::IrInst::DropLocal), and nested
/// [`IrInst::Call`](crate::ir::IrInst::Call) sites may expand to multiple bytecode
/// instructions; VM faults report the function-local PC of the faulting opcode, so every
/// emitted instruction needs a span entry (not only the first byte of the IR lowering).
fn record_instruction_pc_spans(
    pc_spans: &mut Vec<(u32, Span)>,
    base_pc: u32,
    code: &[u8],
    span: Span,
) {
    let mut off = 0usize;
    while off < code.len() {
        pc_spans.push((
            base_pc.saturating_add(u32::try_from(off).unwrap_or(u32::MAX)),
            span,
        ));
        off = match Instruction::decode_at(code, off) {
            Ok((_, next)) => next,
            Err(
                InstrError::Truncated | InstrError::TooManyOperands { .. } | InstrError::Opcode(_),
            ) => break,
        };
    }
}

/// Encodes one [`IrInst`](crate::ir::IrInst) and appends its bytes to `out`.
///
/// Jump operands use precomputed `block_starts`; aggregate and indirect-call operands use
/// `type_remap` when emitting a pruned module type table.
#[allow(clippy::too_many_lines)]
fn emit_inst(
    out: &mut Vec<u8>,
    inst: &IrInst,
    pool: &ConstPoolBuilder,
    def_to_fn: &HashMap<DefId, u32>,
    block_starts: &[u32],
    type_remap: Option<&HashMap<u32, u32>>,
    resolved: &ResolvedProgram,
) -> Result<(), CodegenError> {
    match inst {
        IrInst::Const {
            index, prim_kind, ..
        } => {
            let pool_idx = pool.pool_index_for_literal(*index)?;
            out.extend(encode(Opcode::Const, &[pool_idx, u32::from(*prim_kind)])?);
        }
        IrInst::LoadLocal {
            slot, prim_kind, ..
        } => {
            out.extend(encode(
                Opcode::LoadLocal,
                &[slot.index(), u32::from(*prim_kind)],
            )?);
        }
        IrInst::StoreLocal {
            slot, prim_kind, ..
        } => {
            out.extend(encode(
                Opcode::StoreLocal,
                &[slot.index(), u32::from(*prim_kind)],
            )?);
        }
        IrInst::BinOp { op, prim_kind, .. } => {
            out.extend(encode(ir_binop_to_opcode(*op), &[u32::from(*prim_kind)])?);
        }
        IrInst::Call { callee, .. } => {
            let fn_id = function_id_for(*callee, def_to_fn)?;
            out.extend(encode(Opcode::Call, &[fn_id])?);
        }
        IrInst::MakeFnPtr {
            callee, foreign, ..
        } => {
            let target_kind = u32::from(*foreign);
            let target_id = if *foreign {
                foreign_stub_id(*callee, resolved)?
            } else {
                function_id_for(*callee, def_to_fn)?
            };
            out.extend(encode(Opcode::MakeFnPtr, &[target_kind, target_id])?);
        }
        IrInst::CallIndirect {
            sig_type_id,
            expected_arity,
            ..
        } => {
            let sig = map_type_id(*sig_type_id, type_remap)?;
            out.extend(encode(Opcode::CallIndirect, &[*expected_arity, sig])?);
        }
        IrInst::Return { .. } => {
            out.extend(encode(Opcode::Return, &[])?);
        }
        IrInst::Jump { target } => {
            let off = block_offset(*target, block_starts)?;
            out.extend(encode(Opcode::Jump, &[off])?);
        }
        IrInst::JumpIf {
            then_block,
            else_block,
        } => {
            let then_off = block_offset(*then_block, block_starts)?;
            let else_off = block_offset(*else_block, block_starts)?;
            out.extend(encode(Opcode::JumpIfTrue, &[then_off])?);
            out.extend(encode(Opcode::Jump, &[else_off])?);
        }
        IrInst::MakeStruct {
            type_id,
            field_count,
        } => {
            let ty = map_type_id(*type_id, type_remap)?;
            out.extend(encode(Opcode::MakeStruct, &[ty, *field_count])?);
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
            )?);
        }
        IrInst::GetField {
            type_id,
            field_index,
            ..
        } => {
            let ty = map_type_id(*type_id, type_remap)?;
            out.extend(encode(Opcode::GetField, &[ty, *field_index])?);
        }
        IrInst::SetField {
            type_id,
            field_index,
        } => {
            let ty = map_type_id(*type_id, type_remap)?;
            out.extend(encode(Opcode::SetField, &[ty, *field_index])?);
        }
        IrInst::MatchTag {
            type_id,
            variant_tag,
        } => {
            let ty = map_type_id(*type_id, type_remap)?;
            out.extend(encode(Opcode::MatchTag, &[ty, *variant_tag])?);
        }
        IrInst::Cast { from_kind, to_kind } => {
            out.extend(encode(
                Opcode::Cast,
                &[u32::from(*from_kind), u32::from(*to_kind)],
            )?);
        }
        IrInst::Neg { prim_kind, .. } => {
            out.extend(encode(Opcode::Neg, &[u32::from(*prim_kind)])?);
        }
        IrInst::Not { prim_kind, .. } => {
            out.extend(encode(Opcode::Not, &[u32::from(*prim_kind)])?);
        }
        IrInst::BitNot { prim_kind, .. } => {
            out.extend(encode(Opcode::BitNot, &[u32::from(*prim_kind)])?);
        }
        IrInst::MakeTuple { arity } => {
            out.extend(encode(Opcode::MakeTuple, &[*arity])?);
        }
        IrInst::MakeArray { len } => {
            out.extend(encode(Opcode::MakeArray, &[*len])?);
        }
        IrInst::Index { .. } => {
            out.extend(encode(Opcode::Index, &[])?);
        }
        IrInst::IndexStore {
            prim_kind, signed, ..
        } => {
            out.extend(encode(
                Opcode::IndexStore,
                &[u32::from(*prim_kind), u32::from(*signed)],
            )?);
        }
        IrInst::PtrLoad {
            prim_kind, signed, ..
        } => {
            out.extend(encode(
                Opcode::PtrLoad,
                &[u32::from(*prim_kind), u32::from(*signed)],
            )?);
        }
        IrInst::AddressOfLocal { slot } => {
            out.extend(encode(Opcode::AddressOfLocal, &[slot.index()])?);
        }
        IrInst::LoadAggViaLocalPtr => {
            out.extend(encode(Opcode::LoadAggViaLocalPtr, &[])?);
        }
        IrInst::Alloc { .. } => {
            out.extend(encode(Opcode::Alloc, &[])?);
        }
        IrInst::PtrStore {
            prim_kind, signed, ..
        } => {
            out.extend(encode(
                Opcode::PtrStore,
                &[u32::from(*prim_kind), u32::from(*signed)],
            )?);
        }
        IrInst::Free => {
            out.extend(encode(Opcode::Free, &[])?);
        }
        IrInst::Pop => {
            out.extend(encode(Opcode::Pop, &[])?);
        }
        IrInst::MakeSlice { elem_kind } => {
            out.extend(encode(Opcode::MakeSlice, &[u32::from(*elem_kind)])?);
        }
        IrInst::MakeSliceFromPtr { elem_kind } => {
            out.extend(encode(Opcode::MakeSliceFromPtr, &[u32::from(*elem_kind)])?);
        }
        IrInst::MakeStr { pool_index } => {
            let pool_idx = pool.pool_index_for_literal(*pool_index)?;
            out.extend(encode(Opcode::MakeStr, &[pool_idx])?);
        }
        IrInst::StrAsSlice => {
            out.extend(encode(Opcode::StrAsSlice, &[])?);
        }
        IrInst::SliceLen => {
            out.extend(encode(Opcode::SliceLen, &[])?);
        }
        IrInst::TrapGivenMismatch => {
            out.extend(encode(Opcode::Trap, &[])?);
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
            )?);
            let fn_id = function_id_for(*drop_fn, def_to_fn)?;
            out.extend(encode(Opcode::Call, &[fn_id])?);
            out.extend(encode(Opcode::Pop, &[])?);
        }
    }
    Ok(())
}

/// Builds and encodes a single [`Instruction`](phx_bytecode::Instruction).
fn encode(opcode: Opcode, operands: &[u32]) -> Result<Vec<u8>, CodegenError> {
    Instruction {
        opcode,
        operands: operands.to_vec(),
    }
    .encode()
    .map_err(CodegenError::from)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::ir::{IrBasicBlock, IrFunction, IrFunctionId, IrInst, LocalSlot, SpannedInst};
    use crate::typeck::TypeId;
    use phx_diagnostics::Span;

    fn span_inst(inst: IrInst) -> SpannedInst {
        SpannedInst::new(Span::new(0, 1), inst)
    }

    fn minimal_func(insts: Vec<IrInst>) -> IrFunction {
        IrFunction {
            id: IrFunctionId::from_raw(0),
            def: DefId::from_raw(0),
            params: Vec::new(),
            return_type: TypeId::from_raw(0),
            local_count: 0,
            blocks: vec![IrBasicBlock {
                insts: insts.into_iter().map(span_inst).collect(),
            }],
        }
    }

    fn empty_resolved() -> ResolvedProgram {
        let file = phx_syntax::parse("main :: () => {};");
        let file = file.value;
        crate::resolver::resolve(&file).expect("resolve empty")
    }

    #[test]
    fn emit_fails_on_invalid_jump_block() {
        let func = minimal_func(vec![IrInst::Jump { target: 99 }]);
        let mut pool = ConstPoolBuilder::new();
        let def_to_fn = HashMap::new();
        let fn_arity = HashMap::new();
        let resolved = empty_resolved();
        match emit_function(&func, &mut pool, &def_to_fn, &fn_arity, None, &resolved) {
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
        let resolved = empty_resolved();
        match emit_function(&func, &mut pool, &def_to_fn, &fn_arity, None, &resolved) {
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
        let resolved = empty_resolved();
        match emit_function(&func, &mut pool, &def_to_fn, &fn_arity, None, &resolved) {
            Err(CodegenError::MissingCallee { def_index: 7 }) => {}
            other => panic!("expected MissingCallee {{ def_index: 7 }}, got {other:?}"),
        }
    }

    #[test]
    fn drop_local_stack_effect_is_balanced() {
        use crate::ir::apply_ir_stack_effect_emit;

        let mut stack = 0u32;
        let mut def_to_fn = HashMap::new();
        def_to_fn.insert(DefId::from_raw(1), 0);
        let mut fn_arity = HashMap::new();
        fn_arity.insert(0, 1);
        apply_ir_stack_effect_emit(
            &IrInst::DropLocal {
                slot: LocalSlot::from_raw(0),
                ty: TypeId::from_raw(0),
                drop_fn: DefId::from_raw(1),
                prim_kind: 0,
            },
            &mut stack,
            &def_to_fn,
            &fn_arity,
        )
        .expect("drop local stack effect");
        assert_eq!(stack, 0);
    }

    #[test]
    fn jump_if_records_pc_span_for_each_emitted_instruction() {
        let func = minimal_func(vec![IrInst::JumpIf {
            then_block: 0,
            else_block: 0,
        }]);
        let mut pool = ConstPoolBuilder::new();
        let def_to_fn = HashMap::new();
        let fn_arity = HashMap::new();
        let resolved = empty_resolved();
        let emitted = emit_function(&func, &mut pool, &def_to_fn, &fn_arity, None, &resolved)
            .expect("emit jump if");
        assert_eq!(
            emitted.pc_spans.len(),
            2,
            "JumpIf lowers to JumpIfTrue + Jump"
        );
    }

    #[test]
    fn drop_local_records_pc_span_for_each_emitted_instruction() {
        let func = minimal_func(vec![IrInst::DropLocal {
            slot: LocalSlot::from_raw(0),
            ty: TypeId::from_raw(0),
            drop_fn: DefId::from_raw(1),
            prim_kind: 0,
        }]);
        let mut def_to_fn = HashMap::new();
        def_to_fn.insert(DefId::from_raw(1), 0);
        let fn_arity = HashMap::from([(0u32, 1u16)]);
        let mut pool = ConstPoolBuilder::new();
        let resolved = empty_resolved();
        let emitted = emit_function(&func, &mut pool, &def_to_fn, &fn_arity, None, &resolved)
            .expect("emit drop local");
        assert_eq!(
            emitted.pc_spans.len(),
            3,
            "DropLocal lowers to LoadLocal + Call + Pop"
        );
    }

    #[test]
    fn drop_local_emits_load_call_pop() {
        let func = minimal_func(vec![IrInst::DropLocal {
            slot: LocalSlot::from_raw(0),
            ty: TypeId::from_raw(0),
            drop_fn: DefId::from_raw(1),
            prim_kind: 0,
        }]);
        let mut def_to_fn = HashMap::new();
        def_to_fn.insert(DefId::from_raw(1), 0);
        let fn_arity = HashMap::from([(0u32, 1u16)]);
        let mut pool = ConstPoolBuilder::new();
        let resolved = empty_resolved();
        let emitted = emit_function(&func, &mut pool, &def_to_fn, &fn_arity, None, &resolved)
            .expect("emit drop local");
        let mut opcodes = Vec::new();
        let mut off = 0usize;
        while off < emitted.code.len() {
            let (inst, next) = Instruction::decode_at(&emitted.code, off).expect("decode");
            opcodes.push(inst.opcode);
            off = next;
        }
        assert_eq!(opcodes, vec![Opcode::LoadLocal, Opcode::Call, Opcode::Pop]);
    }
}

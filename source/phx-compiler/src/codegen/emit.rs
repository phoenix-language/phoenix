//! Emit IR CFG to flat PHX0 instruction bytes.
//!
//! ## Stack convention
//!
//! Operands are evaluated left-to-right (bottom = left, top = right). Binary ops pop `b`,
//! then `a`, and push `op(a, b)`. Call leaves `arity` arguments on the stack (bottom = first param).

use phx_bytecode::{Instruction, Opcode, apply_stack_effect};

use crate::ir::{IrBinOp, IrFunction, IrInst};
use crate::resolver::DefId;

use super::const_pool::ConstPoolBuilder;

/// Result of emitting one function body.
#[derive(Debug)]
pub struct EmittedFunction {
    /// Encoded instructions for this function.
    pub code: Vec<u8>,
    /// Maximum operand stack depth observed during emission.
    pub stack_max: u16,
}

/// Emits one function's CFG to bytecode bytes.
#[must_use]
pub fn emit_function(
    func: &IrFunction,
    pool: &mut ConstPoolBuilder,
    def_to_fn: &std::collections::HashMap<DefId, u32>,
    fn_arity: &std::collections::HashMap<u32, u16>,
) -> EmittedFunction {
    pool.collect_from_function(func);
    let block_starts = compute_block_starts(func);
    let (code, stack_max) = emit_blocks(func, pool, def_to_fn, fn_arity, &block_starts);
    EmittedFunction { code, stack_max }
}

impl ConstPoolBuilder {
    /// Walks all instructions in `func` to intern constants.
    pub fn collect_from_function(&mut self, func: &IrFunction) {
        for block in &func.blocks {
            self.collect_insts(&block.insts);
        }
    }
}

fn encoded_size(inst: &IrInst) -> u32 {
    fn with_operands(count: u32) -> u32 {
        2 + count.saturating_mul(4)
    }
    match inst {
        IrInst::JumpIf { .. } => with_operands(1).saturating_add(with_operands(1)),
        IrInst::BinOp { .. } | IrInst::Return { .. } => 2,
        IrInst::Const { .. }
        | IrInst::LoadLocal { .. }
        | IrInst::StoreLocal { .. }
        | IrInst::Call { .. }
        | IrInst::Jump { .. } => with_operands(1),
        IrInst::MakeStruct { .. } | IrInst::GetField { .. } | IrInst::SetField { .. } => {
            with_operands(2)
        }
        IrInst::MakeEnum { .. } => with_operands(3),
        IrInst::MatchTag { .. } => with_operands(2),
    }
}

fn compute_block_starts(func: &IrFunction) -> Vec<u32> {
    let n = func.blocks.len();
    let mut starts = vec![0u32; n];
    let mut offset = 0u32;
    for (block_id, block) in func.blocks.iter().enumerate() {
        starts[block_id] = offset;
        for inst in &block.insts {
            offset = offset.saturating_add(encoded_size(inst));
        }
    }
    starts
}

fn apply_ir_stack_effect(
    inst: &IrInst,
    stack: &mut u32,
    def_to_fn: &std::collections::HashMap<DefId, u32>,
    fn_arity: &std::collections::HashMap<u32, u16>,
) {
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
        IrInst::BinOp { .. } => {
            let _ = apply_stack_effect(Opcode::Add, stack, None, none);
        }
        IrInst::Call { callee, .. } => {
            let fn_id = def_to_fn.get(callee).copied().unwrap_or(0);
            let arity = *fn_arity.get(&fn_id).unwrap_or(&0);
            let _ = apply_stack_effect(Opcode::Call, stack, Some(arity), none);
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
        IrInst::Return { .. } | IrInst::Jump { .. } => {}
    }
}

fn emit_blocks(
    func: &IrFunction,
    pool: &mut ConstPoolBuilder,
    def_to_fn: &std::collections::HashMap<DefId, u32>,
    fn_arity: &std::collections::HashMap<u32, u16>,
    block_starts: &[u32],
) -> (Vec<u8>, u16) {
    let mut out = Vec::new();
    let mut max_stack = 0u32;
    let mut stack = 0u32;
    for block in &func.blocks {
        for inst in &block.insts {
            apply_ir_stack_effect(inst, &mut stack, def_to_fn, fn_arity);
            max_stack = max_stack.max(stack);
            emit_inst(&mut out, inst, pool, def_to_fn, block_starts);
        }
    }
    let stack_max = u16::try_from(max_stack).unwrap_or(u16::MAX);
    (out, stack_max)
}

fn emit_inst(
    out: &mut Vec<u8>,
    inst: &IrInst,
    pool: &mut ConstPoolBuilder,
    def_to_fn: &std::collections::HashMap<DefId, u32>,
    block_starts: &[u32],
) {
    match inst {
        IrInst::Const { index, .. } => {
            let pool_idx = pool.intern_raw(*index);
            out.extend(encode(Opcode::Const, &[pool_idx]));
        }
        IrInst::LoadLocal { slot, .. } => {
            out.extend(encode(Opcode::LoadLocal, &[slot.index()]));
        }
        IrInst::StoreLocal { slot, .. } => {
            out.extend(encode(Opcode::StoreLocal, &[slot.index()]));
        }
        IrInst::BinOp { op, .. } => {
            let opcode = match op {
                IrBinOp::Add => Opcode::Add,
                IrBinOp::Sub => Opcode::Sub,
                IrBinOp::Mul => Opcode::Mul,
                IrBinOp::Div => Opcode::Div,
                IrBinOp::Eq => Opcode::Eq,
                IrBinOp::Lt => Opcode::Lt,
            };
            out.extend(encode(opcode, &[]));
        }
        IrInst::Call { callee, .. } => {
            let fn_id = def_to_fn.get(callee).copied().unwrap_or(0);
            out.extend(encode(Opcode::Call, &[fn_id]));
        }
        IrInst::Return { .. } => {
            out.extend(encode(Opcode::Return, &[]));
        }
        IrInst::Jump { target } => {
            let off = block_starts.get(*target as usize).copied().unwrap_or(0);
            out.extend(encode(Opcode::Jump, &[off]));
        }
        IrInst::JumpIf {
            then_block,
            else_block,
        } => {
            let then_off = block_starts.get(*then_block as usize).copied().unwrap_or(0);
            let else_off = block_starts.get(*else_block as usize).copied().unwrap_or(0);
            out.extend(encode(Opcode::JumpIfTrue, &[then_off]));
            out.extend(encode(Opcode::Jump, &[else_off]));
        }
        IrInst::MakeStruct {
            type_id,
            field_count,
        } => {
            out.extend(encode(Opcode::MakeStruct, &[*type_id, *field_count]));
        }
        IrInst::MakeEnum {
            type_id,
            variant_tag,
            payload_count,
        } => {
            out.extend(encode(
                Opcode::MakeEnum,
                &[*type_id, *variant_tag, *payload_count],
            ));
        }
        IrInst::GetField {
            type_id,
            field_index,
            ..
        } => {
            out.extend(encode(Opcode::GetField, &[*type_id, *field_index]));
        }
        IrInst::SetField {
            type_id,
            field_index,
        } => {
            out.extend(encode(Opcode::SetField, &[*type_id, *field_index]));
        }
        IrInst::MatchTag {
            type_id,
            variant_tag,
        } => {
            out.extend(encode(Opcode::MatchTag, &[*type_id, *variant_tag]));
        }
    }
}

fn encode(opcode: Opcode, operands: &[u32]) -> Vec<u8> {
    Instruction {
        opcode,
        operands: operands.to_vec(),
    }
    .encode()
}

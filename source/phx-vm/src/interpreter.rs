//! Opcode interpreter for one PHX0 module.

use phx_bytecode::{
    BytecodeModule, ConstTag, FunctionRecord, InstrError, Instruction, Opcode,
};

use crate::frame::{Machine, Value};
use crate::VmError;

/// Runs `module` starting at `entry` until `main` returns.
///
/// # Errors
///
/// Returns [`VmError`] on invalid bytecode or unsupported opcodes.
pub fn interpret(module: &BytecodeModule) -> Result<(), VmError> {
    let entry_id = module.header.entry_function_id;
    let entry = find_function(module, entry_id).ok_or(VmError::MissingEntry)?;
    if entry.arity != 0 {
        return Err(VmError::EntryArityNotZero);
    }

    let mut machine = Machine::default();
    machine.push_frame(entry_id, entry.local_count);

    while let Some(frame) = machine.frames.last() {
        if machine.frames.is_empty() {
            break;
        }
        let fn_id = frame.function_id;
        let pc = frame.pc;
        let rec = find_function(module, fn_id).ok_or(VmError::InvalidFunctionId(fn_id))?;
        let code = function_code(module, rec);
        let pc_usize = usize::try_from(pc).unwrap_or(0);
        if pc_usize >= code.len() {
            if machine.frames.len() == 1 {
                return Ok(());
            }
            return Err(VmError::TruncatedCode);
        }

        let (inst, next_pc) = Instruction::decode_at(code, pc_usize).map_err(|e| match e {
            InstrError::Truncated => VmError::TruncatedCode,
            InstrError::Opcode(phx_bytecode::OpcodeError::Unknown(op)) => {
                VmError::UnsupportedOpcode(op)
            }
        })?;

        let frame = machine.frames.last_mut().expect("frame");
        frame.pc = u32::try_from(next_pc).unwrap_or(u32::MAX);

        match inst.opcode {
            Opcode::Const => {
                let idx = inst.operands.first().copied().unwrap_or(0) as usize;
                let v = load_const(module, idx)?;
                machine.stack.push(v);
            }
            Opcode::LoadLocal => {
                let slot = inst.operands.first().copied().unwrap_or(0) as usize;
                let v = *frame
                    .locals
                    .get(slot)
                    .ok_or(VmError::InvalidLocalSlot(slot as u32))?;
                machine.stack.push(v);
            }
            Opcode::StoreLocal => {
                let slot = inst.operands.first().copied().unwrap_or(0) as usize;
                let v = machine.stack.pop().ok_or(VmError::StackUnderflow)?;
                let local = frame
                    .locals
                    .get_mut(slot)
                    .ok_or(VmError::InvalidLocalSlot(slot as u32))?;
                *local = v;
            }
            Opcode::Add => binop(&mut machine.stack, |a, b| a.saturating_add(b))?,
            Opcode::Sub => binop(&mut machine.stack, |a, b| a.saturating_sub(b))?,
            Opcode::Mul => binop(&mut machine.stack, |a, b| a.saturating_mul(b))?,
            Opcode::Div => binop_div(&mut machine.stack)?,
            Opcode::Eq => binop(&mut machine.stack, |a, b| i64::from(a == b))?,
            Opcode::Lt => binop(&mut machine.stack, |a, b| i64::from(a < b))?,
            Opcode::Jump => {
                let target = inst.operands.first().copied().unwrap_or(0);
                frame.pc = target;
            }
            Opcode::JumpIfTrue => {
                let target = inst.operands.first().copied().unwrap_or(0);
                let cond = machine.stack.pop().ok_or(VmError::StackUnderflow)?;
                if cond != 0 {
                    frame.pc = target;
                }
            }
            Opcode::JumpIfFalse => {
                let target = inst.operands.first().copied().unwrap_or(0);
                let cond = machine.stack.pop().ok_or(VmError::StackUnderflow)?;
                if cond == 0 {
                    frame.pc = target;
                }
            }
            Opcode::Call => {
                let callee_id = inst.operands.first().copied().unwrap_or(0);
                let callee = find_function(module, callee_id)
                    .ok_or(VmError::InvalidFunctionId(callee_id))?;
                let arity = usize::from(callee.arity);
                if machine.stack.len() < arity {
                    return Err(VmError::StackUnderflow);
                }
                let mut args = vec![0i64; arity];
                for i in (0..arity).rev() {
                    args[i] = machine.stack.pop().expect("checked len");
                }
                machine.push_frame(callee_id, callee.local_count);
                let callee_frame = machine.frames.last_mut().expect("callee");
                for (i, arg) in args.iter().enumerate() {
                    if let Some(slot) = callee_frame.locals.get_mut(i) {
                        *slot = *arg;
                    }
                }
            }
            Opcode::Return => {
                machine.pop_frame();
                if machine.frames.is_empty() {
                    return Ok(());
                }
            }
            Opcode::Pop => {
                let _ = machine.stack.pop().ok_or(VmError::StackUnderflow)?;
            }
        }
    }
    Ok(())
}

fn find_function(module: &BytecodeModule, id: u32) -> Option<&FunctionRecord> {
    module
        .functions
        .functions
        .iter()
        .find(|f| f.function_id == id)
}

fn function_code<'a>(module: &'a BytecodeModule, rec: &FunctionRecord) -> &'a [u8] {
    let start = rec.code_offset as usize;
    let end = start.saturating_add(rec.code_len as usize);
    module.code.get(start..end).unwrap_or(&[])
}

fn load_const(module: &BytecodeModule, index: usize) -> Result<Value, VmError> {
    let entry = module
        .constants
        .entries
        .get(index)
        .ok_or(VmError::InvalidConstIndex(index as u32))?;
    match entry.tag {
        ConstTag::SignedInt if entry.payload.len() >= 8 => {
            let bytes: [u8; 8] = entry.payload[0..8].try_into().map_err(|_| {
                VmError::InvalidConstPayload
            })?;
            Ok(i64::from_le_bytes(bytes))
        }
        ConstTag::Bool => {
            let b = entry.payload.first().copied().unwrap_or(0);
            Ok(i64::from(b != 0))
        }
        ConstTag::UnsignedInt if entry.payload.len() >= 8 => {
            let bytes: [u8; 8] = entry.payload[0..8].try_into().map_err(|_| {
                VmError::InvalidConstPayload
            })?;
            Ok(i64::from_le_bytes(bytes))
        }
        _ => Err(VmError::InvalidConstPayload),
    }
}

fn binop(stack: &mut Vec<Value>, f: fn(i64, i64) -> i64) -> Result<(), VmError> {
    let b = stack.pop().ok_or(VmError::StackUnderflow)?;
    let a = stack.pop().ok_or(VmError::StackUnderflow)?;
    stack.push(f(a, b));
    Ok(())
}

fn binop_div(stack: &mut Vec<Value>) -> Result<(), VmError> {
    let b = stack.pop().ok_or(VmError::StackUnderflow)?;
    let a = stack.pop().ok_or(VmError::StackUnderflow)?;
    if b == 0 {
        return Err(VmError::DivisionByZero);
    }
    stack.push(a / b);
    Ok(())
}

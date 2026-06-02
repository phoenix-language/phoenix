//! Opcode interpreter for one PHX0 module.

use phx_bytecode::{BytecodeModule, ConstTag, FunctionRecord, InstrError, Instruction, Opcode};

use crate::VmError;
use crate::frame::{Aggregate, Machine, Value};

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
            Opcode::Add => binop_scalar(&mut machine.stack, |a, b| a.saturating_add(b))?,
            Opcode::Sub => binop_scalar(&mut machine.stack, |a, b| a.saturating_sub(b))?,
            Opcode::Mul => binop_scalar(&mut machine.stack, |a, b| a.saturating_mul(b))?,
            Opcode::Div => binop_div(&mut machine.stack)?,
            Opcode::Eq => binop_scalar(&mut machine.stack, |a, b| i64::from(a == b))?,
            Opcode::Lt => binop_scalar(&mut machine.stack, |a, b| i64::from(a < b))?,
            Opcode::Jump => {
                let target = inst.operands.first().copied().unwrap_or(0);
                frame.pc = target;
            }
            Opcode::JumpIfTrue => {
                let target = inst.operands.first().copied().unwrap_or(0);
                let cond = pop_scalar(&mut machine.stack)?;
                if cond != 0 {
                    frame.pc = target;
                }
            }
            Opcode::JumpIfFalse => {
                let target = inst.operands.first().copied().unwrap_or(0);
                let cond = pop_scalar(&mut machine.stack)?;
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
                let mut args = vec![Value::Scalar(0); arity];
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
            Opcode::MakeStruct => {
                let type_id = inst.operands.first().copied().unwrap_or(0);
                let field_count = inst.operands.get(1).copied().unwrap_or(0) as usize;
                let mut fields = Vec::with_capacity(field_count);
                for _ in 0..field_count {
                    fields.push(machine.stack.pop().ok_or(VmError::StackUnderflow)?);
                }
                fields.reverse();
                let handle = machine.push_aggregate(Aggregate::Struct { type_id, fields });
                machine.stack.push(handle);
            }
            Opcode::MakeEnum => {
                let type_id = inst.operands.first().copied().unwrap_or(0);
                let tag = inst.operands.get(1).copied().unwrap_or(0);
                let payload_count = inst.operands.get(2).copied().unwrap_or(0) as usize;
                let mut payload = Vec::with_capacity(payload_count);
                for _ in 0..payload_count {
                    payload.push(machine.stack.pop().ok_or(VmError::StackUnderflow)?);
                }
                payload.reverse();
                let handle = machine.push_aggregate(Aggregate::Enum {
                    type_id,
                    tag,
                    payload,
                });
                machine.stack.push(handle);
            }
            Opcode::GetField => {
                let _type_id = inst.operands.first().copied().unwrap_or(0);
                let field_index = inst.operands.get(1).copied().unwrap_or(0) as usize;
                let agg = machine.stack.pop().ok_or(VmError::StackUnderflow)?;
                let handle = agg.as_agg().ok_or(VmError::InvalidAggregate)?;
                let value = match machine.aggregate(handle) {
                    Some(Aggregate::Struct { fields, .. }) => fields
                        .get(field_index)
                        .copied()
                        .ok_or(VmError::FieldOutOfRange)?,
                    Some(Aggregate::Enum { payload, .. }) => payload
                        .get(field_index)
                        .copied()
                        .ok_or(VmError::FieldOutOfRange)?,
                    None => return Err(VmError::InvalidAggregate),
                };
                machine.stack.push(value);
            }
            Opcode::SetField => {
                let _type_id = inst.operands.first().copied().unwrap_or(0);
                let field_index = inst.operands.get(1).copied().unwrap_or(0) as usize;
                let new_val = machine.stack.pop().ok_or(VmError::StackUnderflow)?;
                let agg = machine.stack.pop().ok_or(VmError::StackUnderflow)?;
                let handle = agg.as_agg().ok_or(VmError::InvalidAggregate)?;
                let agg_ref = machine
                    .aggregate_mut(handle)
                    .ok_or(VmError::InvalidAggregate)?;
                match agg_ref {
                    Aggregate::Struct { fields, .. } => {
                        let slot = fields
                            .get_mut(field_index)
                            .ok_or(VmError::FieldOutOfRange)?;
                        *slot = new_val;
                    }
                    Aggregate::Enum { .. } => return Err(VmError::InvalidAggregate),
                }
                machine.stack.push(agg);
            }
            Opcode::MatchTag => {
                let _type_id = inst.operands.first().copied().unwrap_or(0);
                let expected = inst.operands.get(1).copied().unwrap_or(0);
                let agg = machine.stack.pop().ok_or(VmError::StackUnderflow)?;
                let handle = agg.as_agg().ok_or(VmError::InvalidAggregate)?;
                let matches = match machine.aggregate(handle) {
                    Some(Aggregate::Enum { tag, .. }) => *tag == expected,
                    _ => return Err(VmError::InvalidAggregate),
                };
                machine.stack.push(Value::Scalar(i64::from(matches)));
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
            let bytes: [u8; 8] = entry.payload[0..8]
                .try_into()
                .map_err(|_| VmError::InvalidConstPayload)?;
            Ok(Value::Scalar(i64::from_le_bytes(bytes)))
        }
        ConstTag::Bool => {
            let b = entry.payload.first().copied().unwrap_or(0);
            Ok(Value::Scalar(i64::from(b != 0)))
        }
        ConstTag::UnsignedInt if entry.payload.len() >= 8 => {
            let bytes: [u8; 8] = entry.payload[0..8]
                .try_into()
                .map_err(|_| VmError::InvalidConstPayload)?;
            Ok(Value::Scalar(i64::from_le_bytes(bytes)))
        }
        _ => Err(VmError::InvalidConstPayload),
    }
}

fn pop_scalar(stack: &mut Vec<Value>) -> Result<i64, VmError> {
    match stack.pop().ok_or(VmError::StackUnderflow)? {
        Value::Scalar(v) => Ok(v),
        Value::Agg(_) => Err(VmError::ExpectedScalar),
    }
}

fn binop_scalar(stack: &mut Vec<Value>, f: fn(i64, i64) -> i64) -> Result<(), VmError> {
    let b = pop_scalar(stack)?;
    let a = pop_scalar(stack)?;
    stack.push(Value::Scalar(f(a, b)));
    Ok(())
}

fn binop_div(stack: &mut Vec<Value>) -> Result<(), VmError> {
    let b = pop_scalar(stack)?;
    let a = pop_scalar(stack)?;
    if b == 0 {
        return Err(VmError::DivisionByZero);
    }
    stack.push(Value::Scalar(a / b));
    Ok(())
}

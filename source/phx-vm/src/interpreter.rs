//! Opcode interpreter for one PHX0 module.

use phx_bytecode::{
    BytecodeModule, ConstTag, FunctionRecord, InstrError, Instruction, Opcode, PrimitiveKind,
    ScalarValue,
};

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
            Opcode::Add => binop_add(&mut machine.stack)?,
            Opcode::Sub => binop_sub(&mut machine.stack)?,
            Opcode::Mul => binop_mul(&mut machine.stack)?,
            Opcode::Div => binop_div(&mut machine.stack)?,
            Opcode::Eq => binop_eq(&mut machine.stack)?,
            Opcode::Lt => binop_lt(&mut machine.stack)?,
            Opcode::Jump => {
                let target = inst.operands.first().copied().unwrap_or(0);
                frame.pc = target;
            }
            Opcode::JumpIfTrue => {
                let target = inst.operands.first().copied().unwrap_or(0);
                let cond = pop_scalar_int(&mut machine.stack)?;
                if cond != 0 {
                    frame.pc = target;
                }
            }
            Opcode::JumpIfFalse => {
                let target = inst.operands.first().copied().unwrap_or(0);
                let cond = pop_scalar_int(&mut machine.stack)?;
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
                let mut args = vec![Value::Scalar(ScalarValue::zero_int()); arity];
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
                    Some(Aggregate::Tuple { .. }) | Some(Aggregate::Array { .. }) => {
                        return Err(VmError::InvalidAggregate);
                    }
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
                    Aggregate::Enum { .. } | Aggregate::Tuple { .. } | Aggregate::Array { .. } => {
                        return Err(VmError::InvalidAggregate);
                    }
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
                machine
                    .stack
                    .push(Value::Scalar(ScalarValue::Int(i64::from(matches))));
            }
            Opcode::Cast => {
                let from_byte = inst.operands.first().copied().unwrap_or(0) as u8;
                let to_byte = inst.operands.get(1).copied().unwrap_or(0) as u8;
                let from = PrimitiveKind::from_u8(from_byte).ok_or(VmError::InvalidConstPayload)?;
                let to = PrimitiveKind::from_u8(to_byte).ok_or(VmError::InvalidConstPayload)?;
                let v = pop_scalar_value(&mut machine.stack)?;
                machine
                    .stack
                    .push(Value::Scalar(PrimitiveKind::apply_cast(v, from, to)));
            }
            Opcode::Mod => binop_mod(&mut machine.stack)?,
            Opcode::Pow => binop_pow(&mut machine.stack)?,
            Opcode::Neg => {
                let v = pop_scalar_value(&mut machine.stack)?;
                let out = match v {
                    ScalarValue::Int(n) => ScalarValue::Int(n.wrapping_neg()),
                    ScalarValue::Float(f) => ScalarValue::Float(-f),
                };
                machine.stack.push(Value::Scalar(out));
            }
            Opcode::Not => {
                let v = pop_scalar_value(&mut machine.stack)?;
                let b = match v {
                    ScalarValue::Int(n) => n != 0,
                    ScalarValue::Float(f) => f != 0.0,
                };
                machine
                    .stack
                    .push(Value::Scalar(ScalarValue::Int(i64::from(b))));
            }
            Opcode::BitNot => {
                let v = pop_scalar_int(&mut machine.stack)?;
                machine.stack.push(Value::Scalar(ScalarValue::Int(!v)));
            }
            Opcode::BitAnd => binop_int(&mut machine.stack, |a, b| a & b)?,
            Opcode::BitOr => binop_int(&mut machine.stack, |a, b| a | b)?,
            Opcode::BitXor => binop_int(&mut machine.stack, |a, b| a ^ b)?,
            Opcode::Shl => binop_int(&mut machine.stack, |a, b| a.wrapping_shl(b as u32))?,
            Opcode::Shr => binop_int(&mut machine.stack, |a, b| a.wrapping_shr(b as u32))?,
            Opcode::Ne => binop_cmp(&mut machine.stack, |ord| i64::from(ord != std::cmp::Ordering::Equal))?,
            Opcode::Le => binop_cmp(&mut machine.stack, |ord| {
                i64::from(ord != std::cmp::Ordering::Greater)
            })?,
            Opcode::Ge => binop_cmp(&mut machine.stack, |ord| {
                i64::from(ord != std::cmp::Ordering::Less)
            })?,
            Opcode::MakeTuple => {
                let arity = inst.operands.first().copied().unwrap_or(0) as usize;
                let mut elems = Vec::with_capacity(arity);
                for _ in 0..arity {
                    elems.push(machine.stack.pop().ok_or(VmError::StackUnderflow)?);
                }
                elems.reverse();
                let handle = machine.push_aggregate(Aggregate::Tuple { elems });
                machine.stack.push(handle);
            }
            Opcode::MakeArray => {
                let len = inst.operands.first().copied().unwrap_or(0) as usize;
                let mut elems = Vec::with_capacity(len);
                for _ in 0..len {
                    elems.push(machine.stack.pop().ok_or(VmError::StackUnderflow)?);
                }
                elems.reverse();
                let handle = machine.push_aggregate(Aggregate::Array { elems });
                machine.stack.push(handle);
            }
            Opcode::Alloc => {
                let size = inst.operands.first().copied().unwrap_or(0) as usize;
                let addr = machine.alloc_bytes(size);
                machine.stack.push(Value::Scalar(ScalarValue::Int(addr)));
            }
            Opcode::PtrLoad => {
                let size = inst.operands.first().copied().unwrap_or(0) as u8;
                let signed = inst.operands.get(1).copied().unwrap_or(0) as u8;
                let addr = pop_scalar_int(&mut machine.stack)? as usize;
                let v = read_heap_scalar(&machine.heap, addr, size, signed)?;
                machine.stack.push(Value::Scalar(v));
            }
            Opcode::PtrStore => {
                let size = inst.operands.first().copied().unwrap_or(0) as u8;
                let _signed = inst.operands.get(1).copied().unwrap_or(0) as u8;
                let val = pop_scalar_value(&mut machine.stack)?;
                let addr = pop_scalar_int(&mut machine.stack)? as usize;
                write_heap_scalar(&mut machine.heap, addr, size, val)?;
            }
            Opcode::Index => {
                let index = pop_scalar_int(&mut machine.stack)? as usize;
                let agg = machine.stack.pop().ok_or(VmError::StackUnderflow)?;
                let handle = agg.as_agg().ok_or(VmError::InvalidAggregate)?;
                let value = match machine.aggregate(handle) {
                    Some(Aggregate::Tuple { elems }) | Some(Aggregate::Array { elems }) => {
                        elems.get(index).copied().ok_or(VmError::FieldOutOfRange)?
                    }
                    Some(Aggregate::Struct { .. }) | Some(Aggregate::Enum { .. }) => {
                        return Err(VmError::InvalidAggregate);
                    }
                    None => return Err(VmError::InvalidAggregate),
                };
                machine.stack.push(value);
            }
            Opcode::Trap => {
                let kind = inst.operands.first().copied().unwrap_or(0);
                if kind == 0 {
                    return Err(VmError::GivenMismatch);
                }
                return Err(VmError::UnsupportedOpcode(inst.opcode.as_u8()));
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
            Ok(Value::Scalar(ScalarValue::Int(i64::from_le_bytes(bytes))))
        }
        ConstTag::Bool => {
            let b = entry.payload.first().copied().unwrap_or(0);
            Ok(Value::Scalar(ScalarValue::Int(i64::from(b != 0))))
        }
        ConstTag::UnsignedInt if entry.payload.len() >= 8 => {
            let bytes: [u8; 8] = entry.payload[0..8]
                .try_into()
                .map_err(|_| VmError::InvalidConstPayload)?;
            Ok(Value::Scalar(ScalarValue::Int(i64::from_le_bytes(bytes))))
        }
        ConstTag::Float32 if entry.payload.len() >= 4 => {
            let bytes: [u8; 4] = entry.payload[0..4]
                .try_into()
                .map_err(|_| VmError::InvalidConstPayload)?;
            let bits = u32::from_le_bytes(bytes);
            Ok(Value::Scalar(ScalarValue::Float(f64::from(f32::from_bits(bits)))))
        }
        ConstTag::Float64 if entry.payload.len() >= 8 => {
            let bytes: [u8; 8] = entry.payload[0..8]
                .try_into()
                .map_err(|_| VmError::InvalidConstPayload)?;
            Ok(Value::Scalar(ScalarValue::Float(f64::from_le_bytes(bytes))))
        }
        _ => Err(VmError::InvalidConstPayload),
    }
}

fn pop_scalar_value(stack: &mut Vec<Value>) -> Result<ScalarValue, VmError> {
    match stack.pop().ok_or(VmError::StackUnderflow)? {
        Value::Scalar(v) => Ok(v),
        Value::Agg(_) => Err(VmError::ExpectedScalar),
    }
}

fn pop_scalar_int(stack: &mut Vec<Value>) -> Result<i64, VmError> {
    pop_scalar_value(stack)?.as_int().ok_or(VmError::ExpectedScalar)
}

fn binop_int(stack: &mut Vec<Value>, f: fn(i64, i64) -> i64) -> Result<(), VmError> {
    let b = pop_scalar_int(stack)?;
    let a = pop_scalar_int(stack)?;
    stack.push(Value::Scalar(ScalarValue::Int(f(a, b))));
    Ok(())
}

fn binop_add(stack: &mut Vec<Value>) -> Result<(), VmError> {
    let b = pop_scalar_value(stack)?;
    let a = pop_scalar_value(stack)?;
    let out = match (a, b) {
        (ScalarValue::Int(x), ScalarValue::Int(y)) => ScalarValue::Int(x.saturating_add(y)),
        (ScalarValue::Float(x), ScalarValue::Float(y)) => ScalarValue::Float(x + y),
        _ => return Err(VmError::ExpectedScalar),
    };
    stack.push(Value::Scalar(out));
    Ok(())
}

fn binop_sub(stack: &mut Vec<Value>) -> Result<(), VmError> {
    let b = pop_scalar_value(stack)?;
    let a = pop_scalar_value(stack)?;
    let out = match (a, b) {
        (ScalarValue::Int(x), ScalarValue::Int(y)) => ScalarValue::Int(x.saturating_sub(y)),
        (ScalarValue::Float(x), ScalarValue::Float(y)) => ScalarValue::Float(x - y),
        _ => return Err(VmError::ExpectedScalar),
    };
    stack.push(Value::Scalar(out));
    Ok(())
}

fn binop_mul(stack: &mut Vec<Value>) -> Result<(), VmError> {
    let b = pop_scalar_value(stack)?;
    let a = pop_scalar_value(stack)?;
    let out = match (a, b) {
        (ScalarValue::Int(x), ScalarValue::Int(y)) => ScalarValue::Int(x.saturating_mul(y)),
        (ScalarValue::Float(x), ScalarValue::Float(y)) => ScalarValue::Float(x * y),
        _ => return Err(VmError::ExpectedScalar),
    };
    stack.push(Value::Scalar(out));
    Ok(())
}

fn binop_div(stack: &mut Vec<Value>) -> Result<(), VmError> {
    let b = pop_scalar_value(stack)?;
    let a = pop_scalar_value(stack)?;
    let out = match (a, b) {
        (ScalarValue::Int(x), ScalarValue::Int(y)) => {
            if y == 0 {
                return Err(VmError::DivisionByZero);
            }
            ScalarValue::Int(x / y)
        }
        (ScalarValue::Float(x), ScalarValue::Float(y)) => {
            if y == 0.0 {
                return Err(VmError::DivisionByZero);
            }
            ScalarValue::Float(x / y)
        }
        _ => return Err(VmError::ExpectedScalar),
    };
    stack.push(Value::Scalar(out));
    Ok(())
}

fn binop_mod(stack: &mut Vec<Value>) -> Result<(), VmError> {
    let b = pop_scalar_int(stack)?;
    let a = pop_scalar_int(stack)?;
    if b == 0 {
        return Err(VmError::DivisionByZero);
    }
    stack.push(Value::Scalar(ScalarValue::Int(a % b)));
    Ok(())
}

fn binop_pow(stack: &mut Vec<Value>) -> Result<(), VmError> {
    let exp = pop_scalar_int(stack)?;
    let base = pop_scalar_int(stack)?;
    stack.push(Value::Scalar(ScalarValue::Int(int_pow(base, exp))));
    Ok(())
}

fn cmp_order(a: ScalarValue, b: ScalarValue) -> Result<std::cmp::Ordering, VmError> {
    match (a, b) {
        (ScalarValue::Int(x), ScalarValue::Int(y)) => Ok(x.cmp(&y)),
        (ScalarValue::Float(x), ScalarValue::Float(y)) => Ok(x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)),
        _ => Err(VmError::ExpectedScalar),
    }
}

fn binop_eq(stack: &mut Vec<Value>) -> Result<(), VmError> {
    let b = pop_scalar_value(stack)?;
    let a = pop_scalar_value(stack)?;
    let eq = cmp_order(a, b)? == std::cmp::Ordering::Equal;
    stack.push(Value::Scalar(ScalarValue::Int(i64::from(eq))));
    Ok(())
}

fn binop_lt(stack: &mut Vec<Value>) -> Result<(), VmError> {
    let b = pop_scalar_value(stack)?;
    let a = pop_scalar_value(stack)?;
    let lt = cmp_order(a, b)? == std::cmp::Ordering::Less;
    stack.push(Value::Scalar(ScalarValue::Int(i64::from(lt))));
    Ok(())
}

fn binop_cmp(stack: &mut Vec<Value>, f: fn(std::cmp::Ordering) -> i64) -> Result<(), VmError> {
    let b = pop_scalar_value(stack)?;
    let a = pop_scalar_value(stack)?;
    stack.push(Value::Scalar(ScalarValue::Int(f(cmp_order(a, b)?))));
    Ok(())
}

fn read_heap_scalar(
    heap: &[u8],
    addr: usize,
    size: u8,
    signed: u8,
) -> Result<ScalarValue, VmError> {
    let end = addr.checked_add(usize::from(size)).ok_or(VmError::HeapOutOfBounds)?;
    if end > heap.len() {
        return Err(VmError::HeapOutOfBounds);
    }
    let slice = &heap[addr..end];
    match size {
        1 => {
            let byte = slice[0];
            if signed != 0 {
                Ok(ScalarValue::Int(i8::from_ne_bytes([byte]) as i64))
            } else {
                Ok(ScalarValue::Int(i64::from(byte)))
            }
        }
        2 => {
            let bytes: [u8; 2] = slice.try_into().map_err(|_| VmError::InvalidConstPayload)?;
            if signed != 0 {
                Ok(ScalarValue::Int(i64::from(i16::from_le_bytes(bytes))))
            } else {
                Ok(ScalarValue::Int(i64::from(u16::from_le_bytes(bytes))))
            }
        }
        4 => {
            let bytes: [u8; 4] = slice.try_into().map_err(|_| VmError::InvalidConstPayload)?;
            if signed != 0 {
                Ok(ScalarValue::Int(i64::from(i32::from_le_bytes(bytes))))
            } else {
                let bits = u32::from_le_bytes(bytes);
                Ok(ScalarValue::Int(i64::from(bits)))
            }
        }
        8 => {
            let bytes: [u8; 8] = slice.try_into().map_err(|_| VmError::InvalidConstPayload)?;
            if signed != 0 {
                Ok(ScalarValue::Int(i64::from_le_bytes(bytes)))
            } else {
                Ok(ScalarValue::Int(i64::from_le_bytes(bytes)))
            }
        }
        _ => Err(VmError::InvalidConstPayload),
    }
}

fn write_heap_scalar(heap: &mut Vec<u8>, addr: usize, size: u8, value: ScalarValue) -> Result<(), VmError> {
    let end = addr.checked_add(usize::from(size)).ok_or(VmError::HeapOutOfBounds)?;
    if end > heap.len() {
        return Err(VmError::HeapOutOfBounds);
    }
    match (size, value) {
        (1, ScalarValue::Int(v)) => heap[addr] = v as u8,
        (2, ScalarValue::Int(v)) => heap[addr..end].copy_from_slice(&(v as i16).to_le_bytes()),
        (4, ScalarValue::Int(v)) => heap[addr..end].copy_from_slice(&(v as i32).to_le_bytes()),
        (8, ScalarValue::Int(v)) => heap[addr..end].copy_from_slice(&v.to_le_bytes()),
        (4, ScalarValue::Float(f)) => heap[addr..end].copy_from_slice(&(f as f32).to_le_bytes()),
        (8, ScalarValue::Float(f)) => heap[addr..end].copy_from_slice(&f.to_le_bytes()),
        _ => return Err(VmError::InvalidConstPayload),
    }
    Ok(())
}

fn int_pow(base: i64, exp: i64) -> i64 {
    if exp < 0 {
        return 0;
    }
    if exp == 0 {
        return 1;
    }
    let mut result = 1i64;
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if e & 1 != 0 {
            result = result.saturating_mul(b);
        }
        e >>= 1;
        if e > 0 {
            b = b.saturating_mul(b);
        }
    }
    result
}

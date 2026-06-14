//! Opcode interpreter for one PHX0 module.

use phx_bytecode::{
    BytecodeModule, ConstTag, ENTRY_NONE, FunctionRecord, InstrError, Instruction, Opcode,
    PTR_AGG_TAG, PTR_CONST_TAG, PTR_LOCAL_TAG, PrimitiveKind, ScalarValue, VerifiedModule,
    decode_fn_ptr, fn_ptr_from_id, is_fn_ptr, mask_shift_amount, scalar_from_f64, scalar_from_i128,
    scalar_from_u128, scalar_to_f64, scalar_to_i128, scalar_to_u128,
};

use crate::error::{VmError, VmErrorKind};
use crate::foreign::dispatch_foreign;
use crate::frame::{Aggregate, DEFAULT_HEAP_CAP_BYTES, Machine, Value};

/// Captured VM state when the entry function returns (integration tests only).
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct VmRunCapture {
    /// Local slots for `main` at return.
    pub main_locals: Vec<Value>,
    /// Aggregate arena at return (for struct/enum inspection).
    pub aggregates: Vec<Aggregate>,
    /// Stack value popped at top-level `Return`, if the stack was non-empty.
    pub return_value: Option<Value>,
}

impl VmRunCapture {
    /// Returns the value stored in `main` local slot `index`, if present.
    #[must_use]
    pub fn main_local(&self, index: usize) -> Option<Value> {
        self.main_locals.get(index).copied()
    }
}

/// Runs a verified `module` starting at `entry` until `main` returns.
///
/// # Errors
///
/// Returns [`VmError`] on runtime failure.
pub fn interpret(verified: VerifiedModule<'_>) -> Result<(), VmError> {
    run_captured(verified).map(|_| ())
}

/// Runs a verified `module` and returns `main` local slots captured at entry return.
///
/// Integration-test harness only; production callers use [`interpret`].
///
/// # Errors
///
/// Returns [`VmError`] on runtime failure.
#[doc(hidden)]
pub fn run_captured(verified: VerifiedModule<'_>) -> Result<VmRunCapture, VmError> {
    run_captured_with_heap_cap(verified, DEFAULT_HEAP_CAP_BYTES)
}

/// Runs a verified `module` with a custom heap byte cap (integration / stress tests only).
///
/// # Errors
///
/// Returns [`VmError`] on runtime failure or heap cap exhaustion.
#[doc(hidden)]
pub fn run_captured_with_heap_cap(
    verified: VerifiedModule<'_>,
    heap_cap: usize,
) -> Result<VmRunCapture, VmError> {
    run_captured_unverified_with_heap_cap(verified.module(), heap_cap)
}

fn operand_prim_kind(
    inst: &Instruction,
    operand_index: usize,
) -> Result<PrimitiveKind, VmErrorKind> {
    let byte = inst.operands.get(operand_index).copied().unwrap_or(0) as u8;
    PrimitiveKind::from_u8(byte).ok_or(VmErrorKind::InvalidConstPayload)
}

/// Runs `module` without a verification token (mutation / VM error-path tests only).
///
/// # Errors
///
/// Returns [`VmError`] on invalid bytecode or unsupported opcodes.
#[doc(hidden)]
pub fn interpret_unverified(module: &BytecodeModule) -> Result<(), VmError> {
    run_captured_unverified(module).map(|_| ())
}

/// Runs `module` without a verification token (mutation / VM error-path tests only).
///
/// # Errors
///
/// Returns [`VmError`] on invalid bytecode or unsupported opcodes.
#[doc(hidden)]
pub fn run_captured_unverified(module: &BytecodeModule) -> Result<VmRunCapture, VmError> {
    run_captured_unverified_with_heap_cap(module, DEFAULT_HEAP_CAP_BYTES)
}

/// Runs `module` without a verification token and with a custom heap cap (tests only).
///
/// # Errors
///
/// Returns [`VmError`] on invalid bytecode, unsupported opcodes, or heap cap exhaustion.
#[doc(hidden)]
#[allow(clippy::too_many_lines)]
pub fn run_captured_unverified_with_heap_cap(
    module: &BytecodeModule,
    heap_cap: usize,
) -> Result<VmRunCapture, VmError> {
    let entry_id = module.header.entry_function_id;
    if entry_id == ENTRY_NONE {
        return Err(VmError::without_site(VmErrorKind::NoEntryPoint));
    }
    let entry = find_function(module, entry_id)
        .ok_or(VmErrorKind::MissingEntry)
        .map_err(VmError::without_site)?;
    if entry.arity != 0 {
        return Err(VmError::without_site(VmErrorKind::EntryArityNotZero));
    }

    let mut machine = Machine::with_heap_cap(heap_cap);
    machine.push_frame(entry_id, entry.local_count, &module.local_layouts);

    while let Some(frame) = machine.frames.last() {
        let fn_id = frame.function_id;
        let pc = frame.pc;

        let insn_result: Result<Option<VmRunCapture>, VmErrorKind> = (|| {
            let rec = find_function(module, fn_id).ok_or(VmErrorKind::InvalidFunctionId(fn_id))?;
            let code = function_code(module, rec);
            let pc_usize = usize::try_from(pc).unwrap_or(0);
            if pc_usize >= code.len() {
                return Err(VmErrorKind::TruncatedCode);
            }

            let (inst, next_pc) = Instruction::decode_at(code, pc_usize).map_err(|e| match e {
                InstrError::Truncated | InstrError::TooManyOperands { .. } => {
                    VmErrorKind::TruncatedCode
                }
                InstrError::Opcode(phx_bytecode::OpcodeError::Unknown(op)) => {
                    VmErrorKind::UnsupportedOpcode(op)
                }
            })?;

            if let Some(frame) = machine.frames.last_mut() {
                frame.pc = u32::try_from(next_pc).unwrap_or(u32::MAX);
            }

            match inst.opcode {
                Opcode::Const => {
                    let idx = inst.operands.first().copied().unwrap_or(0) as usize;
                    let kind = operand_prim_kind(&inst, 1)?;
                    let v = load_const(module, idx, kind)?;
                    machine.stack.push(v);
                }
                Opcode::LoadLocal => {
                    let slot = inst.operands.first().copied().unwrap_or(0);
                    let idx =
                        usize::try_from(slot).map_err(|_| VmErrorKind::InvalidLocalSlot(slot))?;
                    let local = machine
                        .frames
                        .last()
                        .and_then(|f| f.locals.get(idx))
                        .ok_or(VmErrorKind::InvalidLocalSlot(slot))?;
                    machine.stack.push(*local);
                }
                Opcode::StoreLocal => {
                    let slot = inst.operands.first().copied().unwrap_or(0);
                    let v = machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
                    let idx =
                        usize::try_from(slot).map_err(|_| VmErrorKind::InvalidLocalSlot(slot))?;
                    let local = machine
                        .frames
                        .last_mut()
                        .and_then(|f| f.locals.get_mut(idx))
                        .ok_or(VmErrorKind::InvalidLocalSlot(slot))?;
                    *local = v;
                }
                Opcode::Add => binop_arith(
                    &mut machine.stack,
                    operand_prim_kind(&inst, 0)?,
                    ArithOp::Add,
                )?,
                Opcode::Sub => binop_arith(
                    &mut machine.stack,
                    operand_prim_kind(&inst, 0)?,
                    ArithOp::Sub,
                )?,
                Opcode::Mul => binop_arith(
                    &mut machine.stack,
                    operand_prim_kind(&inst, 0)?,
                    ArithOp::Mul,
                )?,
                Opcode::Div => binop_arith(
                    &mut machine.stack,
                    operand_prim_kind(&inst, 0)?,
                    ArithOp::Div,
                )?,
                Opcode::Mod => binop_arith(
                    &mut machine.stack,
                    operand_prim_kind(&inst, 0)?,
                    ArithOp::Mod,
                )?,
                Opcode::Pow => binop_arith(
                    &mut machine.stack,
                    operand_prim_kind(&inst, 0)?,
                    ArithOp::Pow,
                )?,
                Opcode::Eq => {
                    binop_cmp(&mut machine.stack, operand_prim_kind(&inst, 0)?, CmpOp::Eq)?;
                }
                Opcode::Lt => {
                    binop_cmp(&mut machine.stack, operand_prim_kind(&inst, 0)?, CmpOp::Lt)?;
                }
                Opcode::Ne => {
                    binop_cmp(&mut machine.stack, operand_prim_kind(&inst, 0)?, CmpOp::Ne)?;
                }
                Opcode::Le => {
                    binop_cmp(&mut machine.stack, operand_prim_kind(&inst, 0)?, CmpOp::Le)?;
                }
                Opcode::Ge => {
                    binop_cmp(&mut machine.stack, operand_prim_kind(&inst, 0)?, CmpOp::Ge)?;
                }
                Opcode::Jump => {
                    let target = inst.operands.first().copied().unwrap_or(0);
                    if let Some(f) = machine.frames.last_mut() {
                        f.pc = target;
                    }
                }
                Opcode::JumpIfTrue => {
                    let target = inst.operands.first().copied().unwrap_or(0);
                    let cond = pop_scalar(&mut machine.stack)?;
                    if cond.is_truthy()
                        && let Some(f) = machine.frames.last_mut()
                    {
                        f.pc = target;
                    }
                }
                Opcode::JumpIfFalse => {
                    let target = inst.operands.first().copied().unwrap_or(0);
                    let cond = pop_scalar(&mut machine.stack)?;
                    if !cond.is_truthy()
                        && let Some(f) = machine.frames.last_mut()
                    {
                        f.pc = target;
                    }
                }
                Opcode::Call => {
                    let callee_id = inst.operands.first().copied().unwrap_or(0);
                    call_phoenix(&mut machine, module, callee_id)?;
                }
                Opcode::Return => {
                    let return_value = machine.stack.pop();
                    let main_locals = if machine.frames.len() == 1 {
                        machine.frames.last().map(|f| f.locals.clone())
                    } else {
                        None
                    };
                    machine.pop_frame();
                    if machine.frames.is_empty() {
                        return Ok(Some(VmRunCapture {
                            main_locals: main_locals.unwrap_or_default(),
                            aggregates: std::mem::take(&mut machine.aggregates),
                            return_value,
                        }));
                    }
                    if let Some(v) = return_value {
                        machine.stack.push(v);
                    }
                }
                Opcode::Pop => {
                    let _ = machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
                }
                Opcode::MakeStruct => {
                    let type_id = inst.operands.first().copied().unwrap_or(0);
                    let field_count = inst.operands.get(1).copied().unwrap_or(0) as usize;
                    let mut fields = Vec::with_capacity(field_count);
                    for _ in 0..field_count {
                        fields.push(machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?);
                    }
                    fields.reverse();
                    let handle = machine.push_aggregate(Aggregate::Struct {
                        _type_id: type_id,
                        fields,
                    });
                    machine.stack.push(handle);
                }
                Opcode::MakeEnum => {
                    let type_id = inst.operands.first().copied().unwrap_or(0);
                    let tag = inst.operands.get(1).copied().unwrap_or(0);
                    let payload_count = inst.operands.get(2).copied().unwrap_or(0) as usize;
                    let mut payload = Vec::with_capacity(payload_count);
                    for _ in 0..payload_count {
                        payload.push(machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?);
                    }
                    payload.reverse();
                    let handle = machine.push_aggregate(Aggregate::Enum {
                        _type_id: type_id,
                        tag,
                        payload,
                    });
                    machine.stack.push(handle);
                }
                Opcode::GetField => {
                    let _type_id = inst.operands.first().copied().unwrap_or(0);
                    let field_index = inst.operands.get(1).copied().unwrap_or(0) as usize;
                    let agg = machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
                    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
                    let value = match machine.aggregate(handle) {
                        Some(Aggregate::Struct { fields, .. }) => fields
                            .get(field_index)
                            .copied()
                            .ok_or(VmErrorKind::FieldOutOfRange)?,
                        Some(Aggregate::Enum { payload, .. }) => payload
                            .get(field_index)
                            .copied()
                            .ok_or(VmErrorKind::FieldOutOfRange)?,
                        Some(
                            Aggregate::Tuple { .. }
                            | Aggregate::Array { .. }
                            | Aggregate::Slice { .. }
                            | Aggregate::Str { .. },
                        ) => {
                            return Err(VmErrorKind::InvalidAggregate);
                        }
                        None => return Err(VmErrorKind::InvalidAggregate),
                    };
                    machine.stack.push(value);
                }
                Opcode::SetField => {
                    let _type_id = inst.operands.first().copied().unwrap_or(0);
                    let field_index = inst.operands.get(1).copied().unwrap_or(0) as usize;
                    let new_val = machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
                    let agg = machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
                    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
                    let agg_ref = machine
                        .aggregate_mut(handle)
                        .ok_or(VmErrorKind::InvalidAggregate)?;
                    match agg_ref {
                        Aggregate::Struct { fields, .. } => {
                            let slot = fields
                                .get_mut(field_index)
                                .ok_or(VmErrorKind::FieldOutOfRange)?;
                            *slot = new_val;
                        }
                        Aggregate::Enum { .. }
                        | Aggregate::Tuple { .. }
                        | Aggregate::Array { .. }
                        | Aggregate::Slice { .. }
                        | Aggregate::Str { .. } => {
                            return Err(VmErrorKind::InvalidAggregate);
                        }
                    }
                    machine.stack.push(agg);
                }
                Opcode::MatchTag => {
                    let _type_id = inst.operands.first().copied().unwrap_or(0);
                    let expected = inst.operands.get(1).copied().unwrap_or(0);
                    let agg = machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
                    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
                    let matches = match machine.aggregate(handle) {
                        Some(Aggregate::Enum { tag, .. }) => *tag == expected,
                        _ => return Err(VmErrorKind::InvalidAggregate),
                    };
                    machine
                        .stack
                        .push(Value::Scalar(ScalarValue::Bool(matches)));
                }
                Opcode::Cast => {
                    let from_byte = inst.operands.first().copied().unwrap_or(0) as u8;
                    let to_byte = inst.operands.get(1).copied().unwrap_or(0) as u8;
                    let from = PrimitiveKind::from_u8(from_byte)
                        .ok_or(VmErrorKind::InvalidConstPayload)?;
                    let to =
                        PrimitiveKind::from_u8(to_byte).ok_or(VmErrorKind::InvalidConstPayload)?;
                    let v = pop_scalar(&mut machine.stack)?;
                    machine
                        .stack
                        .push(Value::Scalar(PrimitiveKind::apply_cast(v, from, to)));
                }
                Opcode::Neg => {
                    let kind = operand_prim_kind(&inst, 0)?;
                    let v = pop_scalar(&mut machine.stack)?;
                    machine.stack.push(Value::Scalar(neg_scalar(v, kind)));
                }
                Opcode::Not => {
                    let v = pop_scalar(&mut machine.stack)?;
                    machine
                        .stack
                        .push(Value::Scalar(ScalarValue::Bool(!v.is_truthy())));
                }
                Opcode::BitNot => {
                    let kind = operand_prim_kind(&inst, 0)?;
                    let v = pop_scalar(&mut machine.stack)?;
                    machine.stack.push(Value::Scalar(bitnot_scalar(v, kind)));
                }
                Opcode::BitAnd => {
                    binop_bit(&mut machine.stack, operand_prim_kind(&inst, 0)?, BitOp::And)?;
                }
                Opcode::BitOr => {
                    binop_bit(&mut machine.stack, operand_prim_kind(&inst, 0)?, BitOp::Or)?;
                }
                Opcode::BitXor => {
                    binop_bit(&mut machine.stack, operand_prim_kind(&inst, 0)?, BitOp::Xor)?;
                }
                Opcode::Shl => {
                    binop_bit(&mut machine.stack, operand_prim_kind(&inst, 0)?, BitOp::Shl)?;
                }
                Opcode::Shr => {
                    binop_bit(&mut machine.stack, operand_prim_kind(&inst, 0)?, BitOp::Shr)?;
                }
                Opcode::MakeTuple => {
                    let arity = inst.operands.first().copied().unwrap_or(0) as usize;
                    let mut elems = Vec::with_capacity(arity);
                    for _ in 0..arity {
                        elems.push(machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?);
                    }
                    elems.reverse();
                    let handle = machine.push_aggregate(Aggregate::Tuple { elems });
                    machine.stack.push(handle);
                }
                Opcode::MakeArray => {
                    let len = inst.operands.first().copied().unwrap_or(0) as usize;
                    let mut elems = Vec::with_capacity(len);
                    for _ in 0..len {
                        elems.push(machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?);
                    }
                    elems.reverse();
                    let handle = machine.push_aggregate(Aggregate::Array { elems });
                    machine.stack.push(handle);
                }
                Opcode::Alloc => {
                    let size_val = pop_scalar(&mut machine.stack)?;
                    let ScalarValue::U32(size) = size_val else {
                        return Err(VmErrorKind::ExpectedScalar);
                    };
                    let addr = machine.alloc_bytes(size as usize)?;
                    machine.stack.push(Value::Scalar(ScalarValue::Ptr(addr)));
                }
                Opcode::Free => {
                    let size_val = pop_scalar(&mut machine.stack)?;
                    let ScalarValue::U32(size) = size_val else {
                        return Err(VmErrorKind::ExpectedScalar);
                    };
                    let ptr_val = pop_scalar(&mut machine.stack)?;
                    let ScalarValue::Ptr(ptr) = ptr_val else {
                        return Err(VmErrorKind::ExpectedScalar);
                    };
                    machine.free_bytes(ptr, size)?;
                }
                Opcode::PtrLoad => {
                    let kind = operand_prim_kind(&inst, 0)?;
                    let signed = inst.operands.get(1).copied().unwrap_or(0) as u8;
                    let addr_val = pop_scalar(&mut machine.stack)?;
                    let ScalarValue::Ptr(ptr) = addr_val else {
                        return Err(VmErrorKind::ExpectedScalar);
                    };
                    let v = ptr_load(&machine, ptr, kind, signed)?;
                    machine.stack.push(Value::Scalar(v));
                }
                Opcode::PtrStore => {
                    let kind = operand_prim_kind(&inst, 0)?;
                    let signed = inst.operands.get(1).copied().unwrap_or(0) as u8;
                    let val = pop_scalar(&mut machine.stack)?;
                    let addr_val = pop_scalar(&mut machine.stack)?;
                    let ScalarValue::Ptr(ptr) = addr_val else {
                        return Err(VmErrorKind::ExpectedScalar);
                    };
                    ptr_store(&mut machine, ptr, kind, signed, val)?;
                }
                Opcode::MakeSlice => {
                    let elem_kind = inst.operands.first().copied().unwrap_or(0) as u8;
                    let agg = machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
                    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
                    let len = match machine.aggregate(handle) {
                        Some(Aggregate::Array { elems }) => u64::try_from(elems.len()).unwrap_or(0),
                        _ => return Err(VmErrorKind::InvalidAggregate),
                    };
                    let ptr = PTR_AGG_TAG | u64::from(handle);
                    let slice = machine.push_aggregate(Aggregate::Slice {
                        elem_kind,
                        ptr,
                        len,
                    });
                    machine.stack.push(slice);
                }
                Opcode::MakeSliceFromPtr => {
                    let elem_kind = inst.operands.first().copied().unwrap_or(0) as u8;
                    let len_val = pop_scalar(&mut machine.stack)?;
                    let ScalarValue::U32(len) = len_val else {
                        return Err(VmErrorKind::ExpectedScalar);
                    };
                    let ptr_val = pop_scalar(&mut machine.stack)?;
                    let ScalarValue::Ptr(ptr) = ptr_val else {
                        return Err(VmErrorKind::ExpectedScalar);
                    };
                    let slice = machine.push_aggregate(Aggregate::Slice {
                        elem_kind,
                        ptr,
                        len: u64::from(len),
                    });
                    machine.stack.push(slice);
                }
                Opcode::AddressOfLocal => {
                    let slot = inst.operands.first().copied().unwrap_or(0);
                    machine
                        .stack
                        .push(Value::Scalar(ScalarValue::local_ptr(slot)));
                }
                Opcode::LoadAggViaLocalPtr => {
                    let ptr = pop_scalar(&mut machine.stack)?;
                    let ScalarValue::Ptr(encoded) = ptr else {
                        return Err(VmErrorKind::ExpectedScalar);
                    };
                    let slot = ScalarValue::local_slot_from_ptr(encoded)
                        .ok_or(VmErrorKind::InvalidConstPayload)?;
                    let frame_idx = machine.frames.len().saturating_sub(2);
                    let frame = machine
                        .frames
                        .get(frame_idx)
                        .ok_or(VmErrorKind::InvalidLocalSlot(slot))?;
                    let idx =
                        usize::try_from(slot).map_err(|_| VmErrorKind::InvalidLocalSlot(slot))?;
                    let local = frame
                        .locals
                        .get(idx)
                        .copied()
                        .ok_or(VmErrorKind::InvalidLocalSlot(slot))?;
                    let Value::Agg(_) = local else {
                        return Err(VmErrorKind::InvalidAggregate);
                    };
                    machine.stack.push(local);
                }
                Opcode::MakeStr => {
                    let idx = inst.operands.first().copied().unwrap_or(0);
                    let entry = module
                        .constants
                        .entries
                        .get(idx as usize)
                        .ok_or(VmErrorKind::InvalidConstIndex(idx))?;
                    if entry.tag != ConstTag::Bytes {
                        return Err(VmErrorKind::InvalidConstPayload);
                    }
                    let len = u64::try_from(entry.payload.len()).unwrap_or(0);
                    let ptr = PTR_CONST_TAG | u64::from(idx);
                    let handle = machine.push_aggregate(Aggregate::Str { ptr, len });
                    machine.stack.push(handle);
                }
                Opcode::StrAsSlice => {
                    let agg = machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
                    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
                    let Aggregate::Str { ptr, len } = machine
                        .aggregate(handle)
                        .ok_or(VmErrorKind::InvalidAggregate)?
                        .clone()
                    else {
                        return Err(VmErrorKind::InvalidAggregate);
                    };
                    let slice = machine.push_aggregate(Aggregate::Slice {
                        elem_kind: PrimitiveKind::U8.as_u8(),
                        ptr,
                        len,
                    });
                    machine.stack.push(slice);
                }
                Opcode::Index => {
                    let index = pop_scalar(&mut machine.stack)?;
                    let idx = scalar_to_usize(index)?;
                    let agg = machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
                    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
                    let value = {
                        let agg_ref = machine
                            .aggregate(handle)
                            .ok_or(VmErrorKind::InvalidAggregate)?;
                        match agg_ref {
                            Aggregate::Tuple { elems } | Aggregate::Array { elems } => elems
                                .get(idx)
                                .copied()
                                .ok_or(VmErrorKind::FieldOutOfRange)?,
                            Aggregate::Slice {
                                elem_kind,
                                ptr,
                                len,
                            } => {
                                if idx >= usize::try_from(*len).unwrap_or(0) {
                                    return Err(VmErrorKind::FieldOutOfRange);
                                }
                                slice_elem_load(module, &machine, *elem_kind, *ptr, idx)?
                            }
                            Aggregate::Str { .. } => return Err(VmErrorKind::InvalidAggregate),
                            Aggregate::Struct { .. } | Aggregate::Enum { .. } => {
                                return Err(VmErrorKind::InvalidAggregate);
                            }
                        }
                    };
                    machine.stack.push(value);
                }
                Opcode::IndexStore => {
                    let kind = operand_prim_kind(&inst, 0)?;
                    let signed = inst.operands.get(1).copied().unwrap_or(0) as u8;
                    let value = pop_scalar(&mut machine.stack)?;
                    let index = pop_scalar(&mut machine.stack)?;
                    let idx = scalar_to_usize(index)?;
                    let agg = machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
                    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
                    let is_array =
                        matches!(machine.aggregate(handle), Some(Aggregate::Array { .. }));
                    if is_array {
                        slice_elem_store(&mut machine, handle, idx, value)?;
                    } else {
                        let (elem_kind, ptr, len) = match machine.aggregate(handle) {
                            Some(Aggregate::Slice {
                                elem_kind,
                                ptr,
                                len,
                            }) => (*elem_kind, *ptr, *len),
                            _ => return Err(VmErrorKind::InvalidAggregate),
                        };
                        if idx >= usize::try_from(len).unwrap_or(0) {
                            return Err(VmErrorKind::FieldOutOfRange);
                        }
                        slice_elem_store_heap(
                            &mut machine,
                            elem_kind,
                            ptr,
                            idx,
                            kind,
                            signed,
                            value,
                        )?;
                    }
                }
                Opcode::Trap => return Err(VmErrorKind::GivenMismatch),
                Opcode::MakeFnPtr => {
                    let target_kind = inst.operands.first().copied().unwrap_or(0);
                    let target_id = inst.operands.get(1).copied().unwrap_or(0);
                    let ptr = fn_ptr_from_id(target_kind, target_id);
                    machine.stack.push(Value::Scalar(ScalarValue::Ptr(ptr)));
                }
                Opcode::CallIndirect => {
                    let expected_arity = inst.operands.first().copied().unwrap_or(0);
                    let arity =
                        usize::try_from(expected_arity).map_err(|_| VmErrorKind::StackUnderflow)?;
                    if machine.stack.len() < arity.saturating_add(1) {
                        return Err(VmErrorKind::StackUnderflow);
                    }
                    let mut args = Vec::with_capacity(arity);
                    for _ in 0..arity {
                        args.push(machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?);
                    }
                    args.reverse();
                    let fn_ptr_val = pop_scalar(&mut machine.stack)?;
                    let ScalarValue::Ptr(ptr) = fn_ptr_val else {
                        return Err(VmErrorKind::InvalidFnPtr);
                    };
                    if !is_fn_ptr(ptr) {
                        return Err(VmErrorKind::InvalidFnPtr);
                    }
                    let (target_kind, target_id) = decode_fn_ptr(ptr);
                    match target_kind {
                        0 => call_phoenix_with_args(&mut machine, module, target_id, &args)?,
                        1 => {
                            for arg in &args {
                                machine.stack.push(*arg);
                            }
                            dispatch_foreign(target_id, &mut machine, module)?;
                        }
                        _ => return Err(VmErrorKind::InvalidFnPtr),
                    }
                }
            }
            Ok(None)
        })();

        match insn_result {
            Ok(Some(capture)) => return Ok(capture),
            Ok(None) => {}
            Err(kind) => return Err(VmError::at(fn_id, pc, kind)),
        }
    }
    Ok(VmRunCapture {
        main_locals: Vec::new(),
        aggregates: std::mem::take(&mut machine.aggregates),
        return_value: None,
    })
}

fn find_function(module: &BytecodeModule, id: u32) -> Option<&FunctionRecord> {
    module
        .functions
        .functions
        .iter()
        .find(|f| f.function_id == id)
}

fn call_phoenix(
    machine: &mut Machine,
    module: &BytecodeModule,
    callee_id: u32,
) -> Result<(), VmErrorKind> {
    let callee =
        find_function(module, callee_id).ok_or(VmErrorKind::InvalidFunctionId(callee_id))?;
    let arity = usize::from(callee.arity);
    if machine.stack.len() < arity {
        return Err(VmErrorKind::StackUnderflow);
    }
    let mut args = Vec::with_capacity(arity);
    for _ in 0..arity {
        args.push(machine.stack.pop().ok_or(VmErrorKind::StackUnderflow)?);
    }
    args.reverse();
    call_phoenix_with_args(machine, module, callee_id, &args)
}

fn call_phoenix_with_args(
    machine: &mut Machine,
    module: &BytecodeModule,
    callee_id: u32,
    args: &[Value],
) -> Result<(), VmErrorKind> {
    let callee =
        find_function(module, callee_id).ok_or(VmErrorKind::InvalidFunctionId(callee_id))?;
    machine.push_frame(callee_id, callee.local_count, &module.local_layouts);
    let callee_frame = machine
        .frames
        .last_mut()
        .ok_or(VmErrorKind::InvalidFunctionId(callee_id))?;
    for (i, arg) in args.iter().enumerate() {
        if let Some(slot) = callee_frame.locals.get_mut(i) {
            *slot = *arg;
        }
    }
    Ok(())
}

fn function_code<'a>(module: &'a BytecodeModule, rec: &FunctionRecord) -> &'a [u8] {
    let start = rec.code_offset as usize;
    let end = start.saturating_add(rec.code_len as usize);
    module.code.get(start..end).unwrap_or(&[])
}

fn load_const(
    module: &BytecodeModule,
    index: usize,
    kind: PrimitiveKind,
) -> Result<Value, VmErrorKind> {
    let entry = module
        .constants
        .entries
        .get(index)
        .ok_or(VmErrorKind::InvalidConstIndex(index as u32))?;
    match entry.tag {
        ConstTag::SignedInt | ConstTag::UnsignedInt => {
            let v = ScalarValue::from_le_bytes(kind, &entry.payload)
                .ok_or(VmErrorKind::InvalidConstPayload)?;
            Ok(Value::Scalar(v))
        }
        ConstTag::Bool => {
            let v = ScalarValue::from_le_bytes(PrimitiveKind::Bool, &entry.payload)
                .ok_or(VmErrorKind::InvalidConstPayload)?;
            Ok(Value::Scalar(v))
        }
        ConstTag::Float32 => {
            let v = ScalarValue::from_le_bytes(PrimitiveKind::F32, &entry.payload)
                .ok_or(VmErrorKind::InvalidConstPayload)?;
            Ok(Value::Scalar(v))
        }
        ConstTag::Float64 => {
            let v = ScalarValue::from_le_bytes(PrimitiveKind::F64, &entry.payload)
                .ok_or(VmErrorKind::InvalidConstPayload)?;
            Ok(Value::Scalar(v))
        }
        ConstTag::Bytes => Err(VmErrorKind::UnsupportedConst),
    }
}

fn pop_scalar(stack: &mut Vec<Value>) -> Result<ScalarValue, VmErrorKind> {
    match stack.pop().ok_or(VmErrorKind::StackUnderflow)? {
        Value::Scalar(v) => Ok(v),
        Value::Agg(_) => Err(VmErrorKind::ExpectedScalar),
    }
}

fn scalar_to_usize(v: ScalarValue) -> Result<usize, VmErrorKind> {
    let n = match v {
        ScalarValue::U8(x) => u64::from(x),
        ScalarValue::U16(x) => u64::from(x),
        ScalarValue::U32(x) => u64::from(x),
        ScalarValue::U64(x) => x,
        ScalarValue::I8(x) => i64::from(x) as u64,
        ScalarValue::I16(x) => i64::from(x) as u64,
        ScalarValue::I32(x) => i64::from(x) as u64,
        ScalarValue::I64(x) => x as u64,
        ScalarValue::I128(x) => x as u64,
        ScalarValue::U128(x) => x as u64,
        ScalarValue::Bool(b) => u64::from(b),
        _ => return Err(VmErrorKind::ExpectedScalar),
    };
    usize::try_from(n).map_err(|_| VmErrorKind::FieldOutOfRange)
}

fn ptr_load(
    machine: &Machine,
    ptr: u64,
    kind: PrimitiveKind,
    signed: u8,
) -> Result<ScalarValue, VmErrorKind> {
    let size = kind.byte_size();
    if ptr & PTR_LOCAL_TAG == PTR_LOCAL_TAG {
        let slot = ScalarValue::local_slot_from_ptr(ptr).ok_or(VmErrorKind::InvalidConstPayload)?;
        let frame = machine
            .frames
            .last()
            .ok_or(VmErrorKind::InvalidLocalSlot(slot))?;
        let bytes = crate::frame::local_scalar_bytes(frame, slot, kind)?;
        return ScalarValue::from_le_bytes(kind, &bytes).ok_or(VmErrorKind::InvalidConstPayload);
    }
    if ptr & PTR_AGG_TAG == PTR_AGG_TAG {
        let handle = (ptr & !PTR_AGG_TAG) as u32;
        return slice_elem_scalar(machine, handle, 0, kind);
    }
    let addr = usize::try_from(ptr).map_err(|_| VmErrorKind::HeapOutOfBounds)?;
    machine.validate_live_heap_access(addr, usize::from(size))?;
    read_heap_scalar(&machine.heap, addr, size, signed, kind)
}

fn ptr_store(
    machine: &mut Machine,
    ptr: u64,
    kind: PrimitiveKind,
    signed: u8,
    value: ScalarValue,
) -> Result<(), VmErrorKind> {
    let size = kind.byte_size();
    if ptr & PTR_LOCAL_TAG == PTR_LOCAL_TAG {
        let slot = ScalarValue::local_slot_from_ptr(ptr).ok_or(VmErrorKind::InvalidConstPayload)?;
        let bytes = value.to_le_bytes(kind);
        let frame = machine
            .frames
            .last_mut()
            .ok_or(VmErrorKind::InvalidLocalSlot(slot))?;
        return crate::frame::store_local_scalar_bytes(frame, slot, kind, &bytes);
    }
    if ptr & PTR_AGG_TAG == PTR_AGG_TAG {
        let handle = (ptr & !PTR_AGG_TAG) as u32;
        return slice_elem_store(machine, handle, 0, value);
    }
    let addr = usize::try_from(ptr).map_err(|_| VmErrorKind::HeapOutOfBounds)?;
    let bytes = scalar_store_bytes(kind, signed, value)?;
    machine.validate_live_heap_access(addr, usize::from(size))?;
    write_heap_scalar(&mut machine.heap, addr, size, &bytes)
}

fn slice_elem_load(
    module: &BytecodeModule,
    machine: &Machine,
    elem_kind: u8,
    ptr: u64,
    index: usize,
) -> Result<Value, VmErrorKind> {
    if ptr & PTR_CONST_TAG == PTR_CONST_TAG {
        let idx =
            u32::try_from(ptr & !PTR_CONST_TAG).map_err(|_| VmErrorKind::InvalidConstPayload)?;
        let bytes = const_pool_bytes(module, idx)?;
        let byte = bytes.get(index).ok_or(VmErrorKind::FieldOutOfRange)?;
        return Ok(Value::Scalar(ScalarValue::U8(*byte)));
    }
    if ptr & PTR_AGG_TAG == PTR_AGG_TAG {
        let handle = (ptr & !PTR_AGG_TAG) as u32;
        if elem_kind == phx_bytecode::SLOT_KIND_AGG {
            let agg = machine
                .aggregate(handle)
                .ok_or(VmErrorKind::InvalidAggregate)?;
            if let Aggregate::Array { elems } = agg {
                return elems
                    .get(index)
                    .copied()
                    .ok_or(VmErrorKind::FieldOutOfRange);
            }
            return Err(VmErrorKind::InvalidAggregate);
        }
        let kind = PrimitiveKind::from_u8(elem_kind).ok_or(VmErrorKind::InvalidConstPayload)?;
        let scalar = slice_elem_scalar(machine, handle, index, kind)?;
        return Ok(Value::Scalar(scalar));
    }
    if ptr & PTR_LOCAL_TAG == PTR_LOCAL_TAG {
        return Err(VmErrorKind::InvalidAggregate);
    }
    if elem_kind == phx_bytecode::SLOT_KIND_AGG {
        return Err(VmErrorKind::InvalidAggregate);
    }
    let kind = PrimitiveKind::from_u8(elem_kind).ok_or(VmErrorKind::InvalidConstPayload)?;
    let elem_size = usize::from(kind.byte_size());
    let addr = usize::try_from(ptr).map_err(|_| VmErrorKind::HeapOutOfBounds)?;
    let byte_offset = index
        .checked_mul(elem_size)
        .ok_or(VmErrorKind::HeapOutOfBounds)?;
    let byte_addr = addr
        .checked_add(byte_offset)
        .ok_or(VmErrorKind::HeapOutOfBounds)?;
    machine.validate_live_heap_access(byte_addr, usize::from(kind.byte_size()))?;
    let scalar = read_heap_scalar(&machine.heap, byte_addr, kind.byte_size(), 0, kind)?;
    Ok(Value::Scalar(scalar))
}

fn const_pool_bytes(module: &BytecodeModule, index: u32) -> Result<&[u8], VmErrorKind> {
    let entry = module
        .constants
        .entries
        .get(index as usize)
        .ok_or(VmErrorKind::InvalidConstIndex(index))?;
    if entry.tag != ConstTag::Bytes {
        return Err(VmErrorKind::InvalidConstPayload);
    }
    Ok(entry.payload.as_slice())
}

fn slice_elem_scalar(
    machine: &Machine,
    handle: u32,
    index: usize,
    kind: PrimitiveKind,
) -> Result<ScalarValue, VmErrorKind> {
    let agg = machine
        .aggregate(handle)
        .ok_or(VmErrorKind::InvalidAggregate)?;
    let elem = match agg {
        Aggregate::Array { elems } => elems.get(index).ok_or(VmErrorKind::FieldOutOfRange)?,
        _ => return Err(VmErrorKind::InvalidAggregate),
    };
    match elem {
        Value::Scalar(s) => {
            if s.primitive_kind() == Some(kind) {
                Ok(*s)
            } else if let Some(k) = s.primitive_kind() {
                Ok(PrimitiveKind::apply_cast(*s, k, kind))
            } else {
                Err(VmErrorKind::ExpectedScalar)
            }
        }
        Value::Agg(_) => Err(VmErrorKind::ExpectedScalar),
    }
}

fn slice_elem_store(
    machine: &mut Machine,
    handle: u32,
    index: usize,
    value: ScalarValue,
) -> Result<(), VmErrorKind> {
    let agg = machine
        .aggregate_mut(handle)
        .ok_or(VmErrorKind::InvalidAggregate)?;
    let slot = match agg {
        Aggregate::Array { elems } => elems.get_mut(index).ok_or(VmErrorKind::FieldOutOfRange)?,
        _ => return Err(VmErrorKind::InvalidAggregate),
    };
    *slot = Value::Scalar(value);
    Ok(())
}

fn slice_elem_store_heap(
    machine: &mut Machine,
    elem_kind: u8,
    ptr: u64,
    index: usize,
    kind: PrimitiveKind,
    signed: u8,
    value: ScalarValue,
) -> Result<(), VmErrorKind> {
    if ptr & PTR_CONST_TAG == PTR_CONST_TAG {
        return Err(VmErrorKind::InvalidAggregate);
    }
    if ptr & PTR_AGG_TAG == PTR_AGG_TAG {
        let handle = (ptr & !PTR_AGG_TAG) as u32;
        return slice_elem_store(machine, handle, index, value);
    }
    if ptr & PTR_LOCAL_TAG == PTR_LOCAL_TAG {
        return Err(VmErrorKind::InvalidAggregate);
    }
    if elem_kind == phx_bytecode::SLOT_KIND_AGG {
        return Err(VmErrorKind::InvalidAggregate);
    }
    let wire_kind = PrimitiveKind::from_u8(elem_kind).ok_or(VmErrorKind::InvalidConstPayload)?;
    if wire_kind != kind {
        return Err(VmErrorKind::InvalidConstPayload);
    }
    let elem_size = usize::from(kind.byte_size());
    let addr = usize::try_from(ptr).map_err(|_| VmErrorKind::HeapOutOfBounds)?;
    let byte_offset = index
        .checked_mul(elem_size)
        .ok_or(VmErrorKind::HeapOutOfBounds)?;
    let byte_addr = addr
        .checked_add(byte_offset)
        .ok_or(VmErrorKind::HeapOutOfBounds)?;
    let bytes = scalar_store_bytes(kind, signed, value)?;
    machine.validate_live_heap_access(byte_addr, usize::from(kind.byte_size()))?;
    write_heap_scalar(&mut machine.heap, byte_addr, kind.byte_size(), &bytes)
}

fn read_heap_scalar(
    heap: &[u8],
    addr: usize,
    size: u8,
    signed: u8,
    kind: PrimitiveKind,
) -> Result<ScalarValue, VmErrorKind> {
    let end = addr
        .checked_add(usize::from(size))
        .ok_or(VmErrorKind::HeapOutOfBounds)?;
    if end > heap.len() {
        return Err(VmErrorKind::HeapOutOfBounds);
    }
    let slice = &heap[addr..end];
    if signed != 0 {
        match size {
            1 => {
                let byte = slice[0];
                let v = i8::from_ne_bytes([byte]);
                return ScalarValue::from_le_bytes(kind, &v.to_le_bytes())
                    .ok_or(VmErrorKind::InvalidConstPayload);
            }
            2 => {
                let b: [u8; 2] = slice
                    .try_into()
                    .map_err(|_| VmErrorKind::InvalidConstPayload)?;
                let v = i16::from_le_bytes(b);
                return ScalarValue::from_le_bytes(kind, &v.to_le_bytes())
                    .ok_or(VmErrorKind::InvalidConstPayload);
            }
            4 => {
                let b: [u8; 4] = slice
                    .try_into()
                    .map_err(|_| VmErrorKind::InvalidConstPayload)?;
                let v = i32::from_le_bytes(b);
                return ScalarValue::from_le_bytes(kind, &v.to_le_bytes())
                    .ok_or(VmErrorKind::InvalidConstPayload);
            }
            8 => {
                let b: [u8; 8] = slice
                    .try_into()
                    .map_err(|_| VmErrorKind::InvalidConstPayload)?;
                let v = i64::from_le_bytes(b);
                return ScalarValue::from_le_bytes(kind, &v.to_le_bytes())
                    .ok_or(VmErrorKind::InvalidConstPayload);
            }
            16 => {
                let b: [u8; 16] = slice
                    .try_into()
                    .map_err(|_| VmErrorKind::InvalidConstPayload)?;
                let v = i128::from_le_bytes(b);
                return ScalarValue::from_le_bytes(kind, &v.to_le_bytes())
                    .ok_or(VmErrorKind::InvalidConstPayload);
            }
            _ => return Err(VmErrorKind::InvalidConstPayload),
        }
    }
    ScalarValue::from_le_bytes(kind, slice).ok_or(VmErrorKind::InvalidConstPayload)
}

/// Encodes `value` for a heap store of width `kind`, honoring signed extension when `signed != 0`.
fn scalar_store_bytes(
    kind: PrimitiveKind,
    signed: u8,
    value: ScalarValue,
) -> Result<Vec<u8>, VmErrorKind> {
    let size = kind.byte_size();
    if signed == 0 {
        let mut bytes = value.to_le_bytes(kind);
        bytes.truncate(usize::from(size));
        return Ok(bytes);
    }
    let wide = match size {
        1 => {
            let v = match value {
                ScalarValue::I8(x) => i64::from(x),
                ScalarValue::U8(x) => i64::from(x),
                ScalarValue::Bool(x) => i64::from(x),
                _ => return Err(VmErrorKind::InvalidConstPayload),
            };
            v.to_le_bytes().to_vec()
        }
        2 => {
            let v = match value {
                ScalarValue::I16(x) => i64::from(x),
                ScalarValue::U16(x) => i64::from(x),
                _ => return Err(VmErrorKind::InvalidConstPayload),
            };
            v.to_le_bytes().to_vec()
        }
        4 => {
            let v = match value {
                ScalarValue::I32(x) => i64::from(x),
                ScalarValue::U32(x) => i64::from(x),
                ScalarValue::F32(x) => return Ok(x.to_le_bytes().to_vec()),
                _ => return Err(VmErrorKind::InvalidConstPayload),
            };
            v.to_le_bytes().to_vec()
        }
        8 => {
            let v = match value {
                ScalarValue::I64(x) => x,
                ScalarValue::U64(x) => x as i64,
                ScalarValue::F32(x) => return Ok(x.to_le_bytes().to_vec()),
                ScalarValue::F64(x) => return Ok(x.to_le_bytes().to_vec()),
                _ => return Err(VmErrorKind::InvalidConstPayload),
            };
            v.to_le_bytes().to_vec()
        }
        16 => {
            let v = match value {
                ScalarValue::I128(x) => x,
                ScalarValue::U128(x) => x as i128,
                _ => return Err(VmErrorKind::InvalidConstPayload),
            };
            v.to_le_bytes().to_vec()
        }
        _ => return Err(VmErrorKind::InvalidConstPayload),
    };
    let start = wide.len().saturating_sub(usize::from(size));
    Ok(wide[start..].to_vec())
}

fn write_heap_scalar(
    heap: &mut [u8],
    addr: usize,
    size: u8,
    bytes: &[u8],
) -> Result<(), VmErrorKind> {
    let end = addr
        .checked_add(usize::from(size))
        .ok_or(VmErrorKind::HeapOutOfBounds)?;
    if end > heap.len() || bytes.len() < usize::from(size) {
        return Err(VmErrorKind::HeapOutOfBounds);
    }
    heap[addr..end].copy_from_slice(&bytes[..usize::from(size)]);
    Ok(())
}

#[derive(Clone, Copy)]
enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

fn binop_arith(
    stack: &mut Vec<Value>,
    kind: PrimitiveKind,
    op: ArithOp,
) -> Result<(), VmErrorKind> {
    let b = pop_scalar(stack)?;
    let a = pop_scalar(stack)?;
    let out = arith_scalar(a, b, kind, op)?;
    stack.push(Value::Scalar(out));
    Ok(())
}

fn arith_scalar(
    a: ScalarValue,
    b: ScalarValue,
    kind: PrimitiveKind,
    op: ArithOp,
) -> Result<ScalarValue, VmErrorKind> {
    if matches!(op, ArithOp::Pow) {
        return Err(VmErrorKind::UnsupportedArithOp);
    }
    if kind.is_float() {
        let af = scalar_to_f64(a, kind);
        let bf = scalar_to_f64(b, kind);
        let out = match op {
            ArithOp::Add => af + bf,
            ArithOp::Sub => af - bf,
            ArithOp::Mul => af * bf,
            ArithOp::Div => {
                if bf == 0.0 {
                    return Err(VmErrorKind::DivisionByZero);
                }
                af / bf
            }
            ArithOp::Mod => {
                if bf == 0.0 {
                    return Err(VmErrorKind::DivisionByZero);
                }
                af % bf
            }
            ArithOp::Pow => return Err(VmErrorKind::UnsupportedArithOp),
        };
        return Ok(scalar_from_f64(out, kind));
    }
    if kind.is_unsigned_int() {
        let au = scalar_to_u128(a, kind);
        let bu = scalar_to_u128(b, kind);
        let out = match op {
            ArithOp::Add => au.wrapping_add(bu),
            ArithOp::Sub => au.wrapping_sub(bu),
            ArithOp::Mul => au.wrapping_mul(bu),
            ArithOp::Div => {
                if bu == 0 {
                    return Err(VmErrorKind::DivisionByZero);
                }
                au / bu
            }
            ArithOp::Mod => {
                if bu == 0 {
                    return Err(VmErrorKind::DivisionByZero);
                }
                au % bu
            }
            ArithOp::Pow => return Err(VmErrorKind::UnsupportedArithOp),
        };
        return Ok(scalar_from_u128(out, kind));
    }
    let ai = scalar_to_i128(a, kind);
    let bi = scalar_to_i128(b, kind);
    let out = match op {
        ArithOp::Add => ai.wrapping_add(bi),
        ArithOp::Sub => ai.wrapping_sub(bi),
        ArithOp::Mul => ai.wrapping_mul(bi),
        ArithOp::Div => {
            if bi == 0 {
                return Err(VmErrorKind::DivisionByZero);
            }
            ai / bi
        }
        ArithOp::Mod => {
            if bi == 0 {
                return Err(VmErrorKind::DivisionByZero);
            }
            ai % bi
        }
        ArithOp::Pow => return Err(VmErrorKind::UnsupportedArithOp),
    };
    Ok(scalar_from_i128(out, kind))
}

#[derive(Clone, Copy)]
enum CmpOp {
    Eq,
    Lt,
    Ne,
    Le,
    Ge,
}

#[allow(clippy::float_cmp)]
fn float_cmp(op: CmpOp, a: f64, b: f64) -> bool {
    match op {
        CmpOp::Eq => a == b,
        CmpOp::Ne => a != b,
        CmpOp::Lt => a < b,
        CmpOp::Le => a <= b,
        CmpOp::Ge => a >= b,
    }
}

fn binop_cmp(stack: &mut Vec<Value>, kind: PrimitiveKind, op: CmpOp) -> Result<(), VmErrorKind> {
    let b = pop_scalar(stack)?;
    let a = pop_scalar(stack)?;
    let result = if kind.is_float() {
        float_cmp(op, scalar_to_f64(a, kind), scalar_to_f64(b, kind))
    } else {
        let ord = if kind.is_unsigned_int() {
            scalar_to_u128(a, kind).cmp(&scalar_to_u128(b, kind))
        } else {
            scalar_to_i128(a, kind).cmp(&scalar_to_i128(b, kind))
        };
        match op {
            CmpOp::Eq => ord == std::cmp::Ordering::Equal,
            CmpOp::Lt => ord == std::cmp::Ordering::Less,
            CmpOp::Ne => ord != std::cmp::Ordering::Equal,
            CmpOp::Le => ord != std::cmp::Ordering::Greater,
            CmpOp::Ge => ord != std::cmp::Ordering::Less,
        }
    };
    stack.push(Value::Scalar(ScalarValue::Bool(result)));
    Ok(())
}

#[derive(Clone, Copy)]
enum BitOp {
    And,
    Or,
    Xor,
    Shl,
    Shr,
}

fn binop_bit(stack: &mut Vec<Value>, kind: PrimitiveKind, op: BitOp) -> Result<(), VmErrorKind> {
    let b = pop_scalar(stack)?;
    let a = pop_scalar(stack)?;
    let shift = mask_shift_amount(scalar_to_u128(b, kind), kind);
    if kind.is_unsigned_int() {
        let au = scalar_to_u128(a, kind);
        let bu = scalar_to_u128(b, kind);
        let out = match op {
            BitOp::And => au & bu,
            BitOp::Or => au | bu,
            BitOp::Xor => au ^ bu,
            BitOp::Shl => au.wrapping_shl(shift),
            BitOp::Shr => au.wrapping_shr(shift),
        };
        stack.push(Value::Scalar(scalar_from_u128(out, kind)));
    } else {
        let ai = scalar_to_i128(a, kind);
        let bi = scalar_to_i128(b, kind);
        let out = match op {
            BitOp::And => ai & bi,
            BitOp::Or => ai | bi,
            BitOp::Xor => ai ^ bi,
            BitOp::Shl => ai.wrapping_shl(shift),
            BitOp::Shr => ai.wrapping_shr(shift),
        };
        stack.push(Value::Scalar(scalar_from_i128(out, kind)));
    }
    Ok(())
}

fn neg_scalar(v: ScalarValue, kind: PrimitiveKind) -> ScalarValue {
    if kind.is_float() {
        return scalar_from_f64(-scalar_to_f64(v, kind), kind);
    }
    if kind.is_unsigned_int() {
        return scalar_from_u128(0u128.wrapping_sub(scalar_to_u128(v, kind)), kind);
    }
    scalar_from_i128(-scalar_to_i128(v, kind), kind)
}

fn bitnot_scalar(v: ScalarValue, kind: PrimitiveKind) -> ScalarValue {
    if kind.is_unsigned_int() {
        scalar_from_u128(!scalar_to_u128(v, kind), kind)
    } else {
        scalar_from_i128(!scalar_to_i128(v, kind), kind)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod scalar_tests {
    use super::*;
    use phx_bytecode::PrimitiveKind;

    #[test]
    fn u128_div_above_i128_max() {
        let top = ScalarValue::U128(1u128 << 127);
        let two = ScalarValue::U128(2);
        let half = arith_scalar(top, two, PrimitiveKind::U128, ArithOp::Div).expect("div");
        assert_eq!(half, ScalarValue::U128(1u128 << 126));
    }

    #[test]
    fn u128_cmp_above_i128_max() {
        let mut stack = vec![
            Value::Scalar(ScalarValue::U128(1u128 << 127)),
            Value::Scalar(ScalarValue::U128(1)),
        ];
        binop_cmp(&mut stack, PrimitiveKind::U128, CmpOp::Ge).expect("cmp");
        let Value::Scalar(ScalarValue::Bool(gt)) = stack.pop().expect("bool") else {
            panic!("expected bool");
        };
        assert!(gt);
    }

    #[test]
    fn float_mod_truncated_remainder() {
        let out = arith_scalar(
            ScalarValue::F64(5.5),
            ScalarValue::F64(2.0),
            PrimitiveKind::F64,
            ArithOp::Mod,
        )
        .expect("mod");
        assert_eq!(out, ScalarValue::F64(1.5));
    }

    #[test]
    fn u8_shift_masks_amount_to_width() {
        let mut stack = vec![
            Value::Scalar(ScalarValue::U8(1)),
            Value::Scalar(ScalarValue::U8(9)),
        ];
        binop_bit(&mut stack, PrimitiveKind::U8, BitOp::Shl).expect("shl");
        let Value::Scalar(ScalarValue::U8(v)) = stack.pop().expect("u8") else {
            panic!("expected u8");
        };
        assert_eq!(v, 2);
    }

    #[test]
    fn u32_shift_masks_full_width_to_zero() {
        let mut stack = vec![
            Value::Scalar(ScalarValue::U32(1)),
            Value::Scalar(ScalarValue::U32(32)),
        ];
        binop_bit(&mut stack, PrimitiveKind::U32, BitOp::Shl).expect("shl");
        let Value::Scalar(ScalarValue::U32(v)) = stack.pop().expect("u32") else {
            panic!("expected u32");
        };
        assert_eq!(v, 1);
    }

    #[test]
    fn nan_eq_nan_is_false() {
        let nan = ScalarValue::F64(f64::NAN);
        let mut stack = vec![Value::Scalar(nan), Value::Scalar(nan)];
        binop_cmp(&mut stack, PrimitiveKind::F64, CmpOp::Eq).expect("eq");
        let Value::Scalar(ScalarValue::Bool(v)) = stack.pop().expect("bool") else {
            panic!("expected bool");
        };
        assert!(!v);
    }

    #[test]
    fn nan_lt_one_is_false() {
        let mut stack = vec![
            Value::Scalar(ScalarValue::F64(f64::NAN)),
            Value::Scalar(ScalarValue::F64(1.0)),
        ];
        binop_cmp(&mut stack, PrimitiveKind::F64, CmpOp::Lt).expect("lt");
        let Value::Scalar(ScalarValue::Bool(v)) = stack.pop().expect("bool") else {
            panic!("expected bool");
        };
        assert!(!v);
    }

    #[test]
    fn nan_le_one_is_false() {
        let mut stack = vec![
            Value::Scalar(ScalarValue::F64(f64::NAN)),
            Value::Scalar(ScalarValue::F64(1.0)),
        ];
        binop_cmp(&mut stack, PrimitiveKind::F64, CmpOp::Le).expect("le");
        let Value::Scalar(ScalarValue::Bool(v)) = stack.pop().expect("bool") else {
            panic!("expected bool");
        };
        assert!(!v);
    }

    #[test]
    fn pow_returns_unsupported() {
        let err = arith_scalar(
            ScalarValue::I32(2),
            ScalarValue::I32(3),
            PrimitiveKind::S32,
            ArithOp::Pow,
        )
        .expect_err("pow");
        assert_eq!(err, VmErrorKind::UnsupportedArithOp);
    }
}

//! Aggregate construction, field access, indexing, and slice opcodes.

use phx_bytecode::{
    BytecodeModule, ConstTag, Instruction, PTR_AGG_TAG, PTR_CONST_TAG, PrimitiveKind, ScalarValue,
};

use crate::VmErrorKind;
use crate::context::{ExecutionContext, VmRuntime};
use crate::frame::{Aggregate, Value};

use super::memory::{read_heap_scalar, scalar_store_bytes, write_heap_scalar};
use super::util::{operand_prim_kind, pop_scalar, scalar_to_usize};

/// Constructs a struct aggregate.
pub(super) fn exec_make_struct(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let type_id = inst.operands.first().copied().unwrap_or(0);
    let field_count = inst.operands.get(1).copied().unwrap_or(0) as usize;
    let mut fields = Vec::with_capacity(field_count);
    for _ in 0..field_count {
        fields.push(ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?);
    }
    fields.reverse();
    let handle = runtime.push_aggregate(Aggregate::Struct {
        _type_id: type_id,
        fields,
    });
    ctx.stack.push(handle);
    Ok(())
}

/// Constructs an enum aggregate.
pub(super) fn exec_make_enum(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let type_id = inst.operands.first().copied().unwrap_or(0);
    let tag = inst.operands.get(1).copied().unwrap_or(0);
    let payload_count = inst.operands.get(2).copied().unwrap_or(0) as usize;
    let mut payload = Vec::with_capacity(payload_count);
    for _ in 0..payload_count {
        payload.push(ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?);
    }
    payload.reverse();
    let handle = runtime.push_aggregate(Aggregate::Enum {
        _type_id: type_id,
        tag,
        payload,
    });
    ctx.stack.push(handle);
    Ok(())
}

/// Reads a struct or enum payload field.
pub(super) fn exec_get_field(
    ctx: &mut ExecutionContext,
    runtime: &VmRuntime,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let _type_id = inst.operands.first().copied().unwrap_or(0);
    let field_index = inst.operands.get(1).copied().unwrap_or(0) as usize;
    let agg = ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
    let value = match runtime.aggregate(handle) {
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
        )
        | None => return Err(VmErrorKind::InvalidAggregate),
    };
    ctx.stack.push(value);
    Ok(())
}

/// Writes a struct field in place.
pub(super) fn exec_set_field(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let _type_id = inst.operands.first().copied().unwrap_or(0);
    let field_index = inst.operands.get(1).copied().unwrap_or(0) as usize;
    let new_val = ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
    let agg = ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
    let agg_ref = runtime
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
        | Aggregate::Str { .. } => return Err(VmErrorKind::InvalidAggregate),
    }
    ctx.stack.push(agg);
    Ok(())
}

/// Compares enum tag against an expected discriminant.
pub(super) fn exec_match_tag(
    ctx: &mut ExecutionContext,
    runtime: &VmRuntime,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let _type_id = inst.operands.first().copied().unwrap_or(0);
    let expected = inst.operands.get(1).copied().unwrap_or(0);
    let agg = ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
    let matches = match runtime.aggregate(handle) {
        Some(Aggregate::Enum { tag, .. }) => *tag == expected,
        _ => return Err(VmErrorKind::InvalidAggregate),
    };
    ctx.stack.push(Value::Scalar(ScalarValue::Bool(matches)));
    Ok(())
}

/// Constructs a tuple aggregate.
pub(super) fn exec_make_tuple(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let arity = inst.operands.first().copied().unwrap_or(0) as usize;
    let mut elems = Vec::with_capacity(arity);
    for _ in 0..arity {
        elems.push(ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?);
    }
    elems.reverse();
    let handle = runtime.push_aggregate(Aggregate::Tuple { elems });
    ctx.stack.push(handle);
    Ok(())
}

/// Constructs a fixed-size array aggregate.
pub(super) fn exec_make_array(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let len = inst.operands.first().copied().unwrap_or(0) as usize;
    let mut elems = Vec::with_capacity(len);
    for _ in 0..len {
        elems.push(ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?);
    }
    elems.reverse();
    let handle = runtime.push_aggregate(Aggregate::Array { elems });
    ctx.stack.push(handle);
    Ok(())
}

/// Wraps an array aggregate as a slice view.
pub(super) fn exec_make_slice(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let elem_kind = inst.operands.first().copied().unwrap_or(0) as u8;
    let agg = ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
    let len = match runtime.aggregate(handle) {
        Some(Aggregate::Array { elems }) => u64::try_from(elems.len()).unwrap_or(0),
        _ => return Err(VmErrorKind::InvalidAggregate),
    };
    let ptr = PTR_AGG_TAG | u64::from(handle);
    let slice = runtime.push_aggregate(Aggregate::Slice {
        elem_kind,
        ptr,
        len,
    });
    ctx.stack.push(slice);
    Ok(())
}

/// Constructs a slice from a raw pointer and length.
pub(super) fn exec_make_slice_from_ptr(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let elem_kind = inst.operands.first().copied().unwrap_or(0) as u8;
    let len_val = pop_scalar(&mut ctx.stack)?;
    let ScalarValue::U32(len) = len_val else {
        return Err(VmErrorKind::ExpectedScalar);
    };
    let ptr_val = pop_scalar(&mut ctx.stack)?;
    let ptr = match ptr_val {
        ScalarValue::Ptr(p) | ScalarValue::U64(p) => p,
        ScalarValue::U32(p) => u64::from(p),
        _ => return Err(VmErrorKind::ExpectedScalar),
    };
    let slice = runtime.push_aggregate(Aggregate::Slice {
        elem_kind,
        ptr,
        len: u64::from(len),
    });
    ctx.stack.push(slice);
    Ok(())
}

/// Materializes a string view over constant-pool bytes.
pub(super) fn exec_make_str(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
    module: &BytecodeModule,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
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
    let handle = runtime.push_aggregate(Aggregate::Str { ptr, len });
    ctx.stack.push(handle);
    Ok(())
}

/// Converts a string aggregate to a byte slice view.
pub(super) fn exec_str_as_slice(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
) -> Result<(), VmErrorKind> {
    let agg = ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
    let Aggregate::Str { ptr, len } = runtime
        .aggregate(handle)
        .ok_or(VmErrorKind::InvalidAggregate)?
        .clone()
    else {
        return Err(VmErrorKind::InvalidAggregate);
    };
    let slice = runtime.push_aggregate(Aggregate::Slice {
        elem_kind: PrimitiveKind::U8.as_u8(),
        ptr,
        len,
    });
    ctx.stack.push(slice);
    Ok(())
}

/// Loads an element by index from tuple, array, or slice.
pub(super) fn exec_index(
    ctx: &mut ExecutionContext,
    runtime: &VmRuntime,
    module: &BytecodeModule,
) -> Result<(), VmErrorKind> {
    let index = pop_scalar(&mut ctx.stack)?;
    let idx = scalar_to_usize(index)?;
    let agg = ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
    let value = {
        let agg_ref = runtime
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
                slice_elem_load(module, runtime, *elem_kind, *ptr, idx)?
            }
            Aggregate::Str { .. } => return Err(VmErrorKind::InvalidAggregate),
            Aggregate::Struct { .. } | Aggregate::Enum { .. } => {
                return Err(VmErrorKind::InvalidAggregate);
            }
        }
    };
    ctx.stack.push(value);
    Ok(())
}

/// Stores an element by index into array or slice backing storage.
pub(super) fn exec_index_store(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let kind = operand_prim_kind(inst, 0)?;
    let signed = inst.operands.get(1).copied().unwrap_or(0) as u8;
    let value = pop_scalar(&mut ctx.stack)?;
    let index = pop_scalar(&mut ctx.stack)?;
    let idx = scalar_to_usize(index)?;
    let agg = ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
    let handle = agg.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
    let is_array = matches!(runtime.aggregate(handle), Some(Aggregate::Array { .. }));
    if is_array {
        slice_elem_store(runtime, handle, idx, value)?;
    } else {
        let (elem_kind, ptr, len) = match runtime.aggregate(handle) {
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
        slice_elem_store_heap(runtime, elem_kind, ptr, idx, kind, signed, value)?;
    }
    Ok(())
}

pub(super) fn slice_elem_load(
    module: &BytecodeModule,
    runtime: &VmRuntime,
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
            let agg = runtime
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
        let scalar = slice_elem_scalar(runtime, handle, index, kind)?;
        return Ok(Value::Scalar(scalar));
    }
    if ptr & phx_bytecode::PTR_LOCAL_TAG == phx_bytecode::PTR_LOCAL_TAG {
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
    runtime.validate_live_heap_access(byte_addr, usize::from(kind.byte_size()))?;
    let scalar = read_heap_scalar(&runtime.heap, byte_addr, kind.byte_size(), 0, kind)?;
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

pub(super) fn slice_elem_scalar(
    runtime: &VmRuntime,
    handle: u32,
    index: usize,
    kind: PrimitiveKind,
) -> Result<ScalarValue, VmErrorKind> {
    let agg = runtime
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

pub(super) fn slice_elem_store(
    runtime: &mut VmRuntime,
    handle: u32,
    index: usize,
    value: ScalarValue,
) -> Result<(), VmErrorKind> {
    let agg = runtime
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
    runtime: &mut VmRuntime,
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
        return slice_elem_store(runtime, handle, index, value);
    }
    if ptr & phx_bytecode::PTR_LOCAL_TAG == phx_bytecode::PTR_LOCAL_TAG {
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
    runtime.validate_live_heap_access(byte_addr, usize::from(kind.byte_size()))?;
    write_heap_scalar(&mut runtime.heap, byte_addr, kind.byte_size(), &bytes)
}

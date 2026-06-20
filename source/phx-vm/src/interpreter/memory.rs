//! Linear-heap allocation and tagged pointer load/store opcodes.
//!
//! [`exec_alloc`] and [`exec_free`] delegate to [`VmRuntime`](crate::context::VmRuntime)'s heap
//! ledger (see [`crate::context`]). Raw pointer ops decode [`PTR_LOCAL_TAG`], [`PTR_AGG_TAG`],
//! and untagged heap addresses in [`ptr_load`] and [`ptr_store`].
//!
//! [`exec_address_of_local`] and [`exec_load_agg_via_local_ptr`] support borrow-by-slot: local
//! pointers encode a slot index and resolve by walking ancestor frames. Aggregate element reads
//! for tagged aggregate pointers delegate to [`super::aggregates`].

use phx_bytecode::{Instruction, PTR_AGG_TAG, PTR_LOCAL_TAG, PrimitiveKind, ScalarValue};

use crate::VmErrorKind;
use crate::context::{ExecutionContext, VmRuntime};
use crate::frame::Value;

use super::util::{operand_prim_kind, pop_scalar};

/// Allocates heap bytes.
///
/// [`Opcode::Alloc`](phx_bytecode::Opcode::Alloc) — stack: `[size: u32] → [addr: ptr]`.
///
/// # Errors
///
/// Returns [`VmErrorKind::ExpectedScalar`] or heap ledger errors from [`VmRuntime::alloc_bytes`].
pub(super) fn exec_alloc(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
) -> Result<(), VmErrorKind> {
    let size_val = pop_scalar(&mut ctx.stack)?;
    let ScalarValue::U32(size) = size_val else {
        return Err(VmErrorKind::ExpectedScalar);
    };
    let addr = runtime.alloc_bytes(size as usize)?;
    ctx.stack.push(Value::Scalar(ScalarValue::Ptr(addr)));
    Ok(())
}

/// Frees a heap block.
pub(super) fn exec_free(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
) -> Result<(), VmErrorKind> {
    let size_val = pop_scalar(&mut ctx.stack)?;
    let ScalarValue::U32(size) = size_val else {
        return Err(VmErrorKind::ExpectedScalar);
    };
    let ptr_val = pop_scalar(&mut ctx.stack)?;
    let ScalarValue::Ptr(ptr) = ptr_val else {
        return Err(VmErrorKind::ExpectedScalar);
    };
    runtime.free_bytes(ptr, size)
}

/// Loads through a raw pointer.
pub(super) fn exec_ptr_load(
    ctx: &mut ExecutionContext,
    runtime: &VmRuntime,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let kind = operand_prim_kind(inst, 0)?;
    let signed = inst.operands.get(1).copied().unwrap_or(0) as u8;
    let addr_val = pop_scalar(&mut ctx.stack)?;
    let ScalarValue::Ptr(ptr) = addr_val else {
        return Err(VmErrorKind::ExpectedScalar);
    };
    let v = ptr_load(ctx, runtime, ptr, kind, signed)?;
    ctx.stack.push(Value::Scalar(v));
    Ok(())
}

/// Stores through a raw pointer.
pub(super) fn exec_ptr_store(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let kind = operand_prim_kind(inst, 0)?;
    let signed = inst.operands.get(1).copied().unwrap_or(0) as u8;
    let val = pop_scalar(&mut ctx.stack)?;
    let addr_val = pop_scalar(&mut ctx.stack)?;
    let ScalarValue::Ptr(ptr) = addr_val else {
        return Err(VmErrorKind::ExpectedScalar);
    };
    ptr_store(ctx, runtime, ptr, kind, signed, val)
}

/// Pushes a tagged local-slot pointer.
pub(super) fn exec_address_of_local(ctx: &mut ExecutionContext, inst: &Instruction) {
    let slot = inst.operands.first().copied().unwrap_or(0);
    ctx.stack.push(Value::Scalar(ScalarValue::local_ptr(slot)));
}

/// Loads an aggregate value through a tagged local pointer (`AddressOfLocal`).
///
/// The pointer encodes only a slot index in the binding's home frame. Callees may
/// re-pass the same pointer through nested calls (`main` → `push` → `grow`), so
/// resolution walks from the immediate caller up to the root frame.
pub(super) fn exec_load_agg_via_local_ptr(ctx: &mut ExecutionContext) -> Result<(), VmErrorKind> {
    let ptr = pop_scalar(&mut ctx.stack)?;
    let ScalarValue::Ptr(encoded) = ptr else {
        return Err(VmErrorKind::ExpectedScalar);
    };
    let slot = ScalarValue::local_slot_from_ptr(encoded).ok_or(VmErrorKind::InvalidConstPayload)?;
    let local = local_aggregate_in_ancestor_frames(ctx, slot)?;
    ctx.stack.push(local);
    Ok(())
}

/// Finds `Value::Agg` at `slot` in the caller frame or any ancestor frame.
fn local_aggregate_in_ancestor_frames(
    ctx: &ExecutionContext,
    slot: u32,
) -> Result<Value, VmErrorKind> {
    let idx = usize::try_from(slot).map_err(|_| VmErrorKind::InvalidLocalSlot(slot))?;
    let caller_idx = ctx.frames.len().saturating_sub(2);
    for fi in (0..=caller_idx).rev() {
        if let Some(local) = ctx.frames.get(fi).and_then(|f| f.locals.get(idx))
            && matches!(local, Value::Agg(_))
        {
            return Ok(*local);
        }
    }
    Err(VmErrorKind::InvalidAggregate)
}

/// Returns `true` when `value` is a tagged local-slot pointer (a borrow cell), not pointee data.
fn is_local_indirection_scalar(value: Value) -> bool {
    matches!(
        value,
        Value::Scalar(ScalarValue::Ptr(ptr)) if ptr & PTR_LOCAL_TAG == PTR_LOCAL_TAG
    )
}

/// Loads a primitive from `slot`, walking from the current frame outward.
///
/// Skips borrow cells that hold `PTR_LOCAL_TAG` pointers so `*param` through a callee
/// slot does not treat the parameter cell as the pointee.
fn local_scalar_in_frames(
    ctx: &ExecutionContext,
    slot: u32,
    kind: PrimitiveKind,
) -> Result<ScalarValue, VmErrorKind> {
    let idx = usize::try_from(slot).map_err(|_| VmErrorKind::InvalidLocalSlot(slot))?;
    for fi in (0..ctx.frames.len()).rev() {
        let Some(frame) = ctx.frames.get(fi) else {
            continue;
        };
        let Some(local) = frame.locals.get(idx) else {
            continue;
        };
        if is_local_indirection_scalar(*local) {
            continue;
        }
        let bytes = crate::frame::local_scalar_bytes(frame, slot, kind)?;
        if let Some(v) = ScalarValue::from_le_bytes(kind, &bytes) {
            return Ok(v);
        }
    }
    Err(VmErrorKind::InvalidLocalSlot(slot))
}

/// Stores primitive bytes into `slot`, walking from the current frame outward.
fn store_local_scalar_in_frames(
    ctx: &mut ExecutionContext,
    slot: u32,
    kind: PrimitiveKind,
    bytes: &[u8],
) -> Result<(), VmErrorKind> {
    let idx = usize::try_from(slot).map_err(|_| VmErrorKind::InvalidLocalSlot(slot))?;
    for fi in (0..ctx.frames.len()).rev() {
        let frame = ctx
            .frames
            .get_mut(fi)
            .ok_or(VmErrorKind::InvalidLocalSlot(slot))?;
        let Some(local) = frame.locals.get(idx) else {
            continue;
        };
        if is_local_indirection_scalar(*local) {
            continue;
        }
        if local.as_scalar().is_some() {
            return crate::frame::store_local_scalar_bytes(frame, slot, kind, bytes);
        }
    }
    Err(VmErrorKind::InvalidLocalSlot(slot))
}

pub(super) fn ptr_load(
    ctx: &ExecutionContext,
    runtime: &VmRuntime,
    ptr: u64,
    kind: PrimitiveKind,
    signed: u8,
) -> Result<ScalarValue, VmErrorKind> {
    let size = kind.byte_size();
    if ptr & PTR_LOCAL_TAG == PTR_LOCAL_TAG {
        let slot = ScalarValue::local_slot_from_ptr(ptr).ok_or(VmErrorKind::InvalidConstPayload)?;
        return local_scalar_in_frames(ctx, slot, kind);
    }
    if ptr & PTR_AGG_TAG == PTR_AGG_TAG {
        let handle = (ptr & !PTR_AGG_TAG) as u32;
        return super::aggregates::slice_elem_scalar(runtime, handle, 0, kind);
    }
    let addr = usize::try_from(ptr).map_err(|_| VmErrorKind::HeapOutOfBounds)?;
    runtime.validate_live_heap_access(addr, usize::from(size))?;
    read_heap_scalar(&runtime.heap, addr, size, signed, kind)
}

pub(super) fn ptr_store(
    ctx: &mut ExecutionContext,
    runtime: &mut VmRuntime,
    ptr: u64,
    kind: PrimitiveKind,
    signed: u8,
    value: ScalarValue,
) -> Result<(), VmErrorKind> {
    let size = kind.byte_size();
    if ptr & PTR_LOCAL_TAG == PTR_LOCAL_TAG {
        let slot = ScalarValue::local_slot_from_ptr(ptr).ok_or(VmErrorKind::InvalidConstPayload)?;
        let bytes = value.to_le_bytes(kind);
        return store_local_scalar_in_frames(ctx, slot, kind, &bytes);
    }
    if ptr & PTR_AGG_TAG == PTR_AGG_TAG {
        let handle = (ptr & !PTR_AGG_TAG) as u32;
        return super::aggregates::slice_elem_store(runtime, handle, 0, value);
    }
    let addr = usize::try_from(ptr).map_err(|_| VmErrorKind::HeapOutOfBounds)?;
    let bytes = scalar_store_bytes(kind, signed, value)?;
    runtime.validate_live_heap_access(addr, usize::from(size))?;
    write_heap_scalar(&mut runtime.heap, addr, size, &bytes)
}

pub(super) fn read_heap_scalar(
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
pub(super) fn scalar_store_bytes(
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
    Ok(wide[..usize::from(size)].to_vec())
}

pub(super) fn write_heap_scalar(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Frame, Value};

    fn ptr_load_ok(
        ctx: &ExecutionContext,
        runtime: &VmRuntime,
        ptr: u64,
        kind: PrimitiveKind,
        signed: u8,
    ) -> ScalarValue {
        match ptr_load(ctx, runtime, ptr, kind, signed) {
            Ok(value) => value,
            Err(kind) => panic!("ptr_load: {kind:?}"),
        }
    }

    #[test]
    fn ptr_load_local_tag_skips_borrow_cell_and_reads_caller_scalar() {
        let mut ctx = ExecutionContext::default();
        ctx.frames.push(Frame {
            function_id: 0,
            pc: 0,
            locals: vec![Value::Scalar(ScalarValue::I32(42))],
        });
        ctx.frames.push(Frame {
            function_id: 1,
            pc: 0,
            locals: vec![Value::Scalar(ScalarValue::local_ptr(0))],
        });
        let runtime = VmRuntime::default();
        let ptr = ScalarValue::local_ptr(0);
        let ScalarValue::Ptr(encoded) = ptr else {
            panic!("expected ptr");
        };
        let loaded = ptr_load_ok(&ctx, &runtime, encoded, PrimitiveKind::S32, 1);
        assert_eq!(loaded, ScalarValue::I32(42));
    }
}

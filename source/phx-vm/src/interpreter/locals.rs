//! Constant-pool and local-slot load/store opcodes.
//!
//! [`exec_const`] materializes rodata from the module constant pool using the instruction's wire
//! kind operand. [`exec_load_local`] and [`exec_store_local`] read/write the active frame's local
//! vector by slot index; out-of-range slots return [`VmErrorKind::InvalidLocalSlot`].
//!
//! Byte-typed constants ([`ConstTag::Bytes`](phx_bytecode::ConstTag::Bytes)) are not yet
//! supported at runtime.
//!
//! Local slots hold arbitrary [`Value`] cells (scalars or aggregate handles). Load/store opcodes
//! copy the cell by value; they do not deep-copy aggregate arena contents.

use phx_bytecode::{BytecodeModule, ConstTag, Instruction, PrimitiveKind, ScalarValue};

use crate::VmErrorKind;
use crate::context::ExecutionContext;
use crate::frame::Value;

/// Loads a constant-pool entry onto the operand stack.
///
/// Operand 0 is the constant-pool index; operand 1 is the wire [`PrimitiveKind`] passed to
/// [`ScalarValue::from_le_bytes`](phx_bytecode::ScalarValue::from_le_bytes) when decoding the
/// entry payload. The verifier ensures the kind matches the pool entry tag; a mismatch at runtime
/// yields [`VmErrorKind::InvalidConstPayload`].
///
/// Stack: `[...] → [..., value]`.
///
/// # Errors
///
/// Returns [`VmErrorKind::InvalidConstIndex`] when the pool index is out of range.
/// Returns [`VmErrorKind::InvalidConstPayload`] when operand 1 is not a valid
/// [`PrimitiveKind`](phx_bytecode::PrimitiveKind) or the payload cannot be decoded for the given
/// kind.
/// Returns [`VmErrorKind::UnsupportedConst`] for [`ConstTag::Bytes`] entries (not implemented in
/// v0).
///
/// # Panics
///
/// Never panics on malformed user bytecode; returns [`VmErrorKind`] instead.
pub(super) fn exec_const(
    ctx: &mut ExecutionContext,
    module: &BytecodeModule,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let idx = inst.operands.first().copied().unwrap_or(0) as usize;
    let kind = super::util::operand_prim_kind(inst, 1)?;
    let v = load_const(module, idx, kind)?;
    ctx.stack.push(v);
    Ok(())
}

/// Copies a local slot from the active frame onto the operand stack.
///
/// Operand 0 is the slot index in the current frame's [`crate::frame::Frame::locals`] vector.
/// The slot value is copied ([`Copy`](std::marker::Copy) for [`Value`]); aggregate handles refer
/// to the same arena entry after the load.
///
/// Stack: `[...] → [..., local]`.
///
/// # Errors
///
/// Returns [`VmErrorKind::InvalidLocalSlot`] when the slot index does not fit in `usize`, the
/// call stack is empty, or the slot is beyond the frame's local vector length.
///
/// # Panics
///
/// Never panics on malformed user bytecode; returns [`VmErrorKind`] instead.
pub(super) fn exec_load_local(
    ctx: &mut ExecutionContext,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let slot = inst.operands.first().copied().unwrap_or(0);
    let idx = usize::try_from(slot).map_err(|_| VmErrorKind::InvalidLocalSlot(slot))?;
    let local = ctx
        .frames
        .last()
        .and_then(|f| f.locals.get(idx))
        .ok_or(VmErrorKind::InvalidLocalSlot(slot))?;
    ctx.stack.push(*local);
    Ok(())
}

/// Pops the stack top and stores it into a local slot on the active frame.
///
/// Operand 0 is the destination slot index. Overwrites the previous cell in place; if the popped
/// value is an aggregate handle, the slot now refers to that handle (shared arena semantics).
///
/// Stack: `[..., value] → [...]`.
///
/// # Errors
///
/// Returns [`VmErrorKind::StackUnderflow`] when the operand stack is empty.
/// Returns [`VmErrorKind::InvalidLocalSlot`] when the slot index is invalid or out of range for
/// the active frame (same rules as [`exec_load_local`]).
///
/// # Panics
///
/// Never panics on malformed user bytecode; returns [`VmErrorKind`] instead.
pub(super) fn exec_store_local(
    ctx: &mut ExecutionContext,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let slot = inst.operands.first().copied().unwrap_or(0);
    let v = ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
    let idx = usize::try_from(slot).map_err(|_| VmErrorKind::InvalidLocalSlot(slot))?;
    let local = ctx
        .frames
        .last_mut()
        .and_then(|f| f.locals.get_mut(idx))
        .ok_or(VmErrorKind::InvalidLocalSlot(slot))?;
    *local = v;
    Ok(())
}

/// Decodes one constant-pool entry into a stack [`Value`].
///
/// Matches on [`ConstTag`](phx_bytecode::ConstTag) to select the decode path. Integer tags use the
/// instruction's wire `kind`; bool and float tags use fixed primitive kinds. Payload length and
/// tag/kind consistency are checked at verify time; runtime decoding failures indicate corrupt
/// images or verifier bypass (unverified test path).
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

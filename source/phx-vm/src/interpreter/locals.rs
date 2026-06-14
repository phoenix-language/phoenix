//! Local slot and constant-pool load/store opcodes.

use phx_bytecode::{BytecodeModule, ConstTag, Instruction, PrimitiveKind, ScalarValue};

use crate::VmErrorKind;
use crate::context::ExecutionContext;
use crate::frame::Value;

/// Loads a constant pool entry onto the stack.
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

/// Loads a local slot onto the stack.
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

/// Stores the stack top into a local slot.
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

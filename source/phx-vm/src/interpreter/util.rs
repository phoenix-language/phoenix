//! Shared stack and operand helpers for opcode handlers.
//!
//! [`operand_prim_kind`] decodes wire-type operands on [`Instruction`](phx_bytecode::Instruction).
//! [`pop_scalar`] enforces scalar-vs-aggregate stack expectations. [`scalar_to_usize`] converts
//! integer scalars for aggregate indexing in [`super::aggregates`].

use phx_bytecode::{Instruction, PrimitiveKind, ScalarValue};

use crate::VmErrorKind;
use crate::frame::Value;

/// Decodes the primitive-kind operand at `operand_index`.
pub(super) fn operand_prim_kind(
    inst: &Instruction,
    operand_index: usize,
) -> Result<PrimitiveKind, VmErrorKind> {
    let byte = inst.operands.get(operand_index).copied().unwrap_or(0) as u8;
    PrimitiveKind::from_u8(byte).ok_or(VmErrorKind::InvalidConstPayload)
}

/// Pops a scalar from the operand stack.
pub(super) fn pop_scalar(stack: &mut Vec<Value>) -> Result<ScalarValue, VmErrorKind> {
    match stack.pop().ok_or(VmErrorKind::StackUnderflow)? {
        Value::Scalar(v) => Ok(v),
        Value::Agg(_) => Err(VmErrorKind::ExpectedScalar),
    }
}

/// Converts an integer scalar to `usize` for indexing.
pub(super) fn scalar_to_usize(v: ScalarValue) -> Result<usize, VmErrorKind> {
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

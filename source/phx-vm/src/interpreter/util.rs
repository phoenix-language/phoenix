//! Shared stack and operand helpers for opcode handlers.
//!
//! [`operand_prim_kind`] decodes wire-type operands on [`Instruction`](phx_bytecode::Instruction).
//! [`pop_scalar`] enforces scalar-vs-aggregate stack expectations. [`scalar_to_usize`] converts
//! integer scalars for aggregate indexing in [`super::aggregates`].
//!
//! Opcode handlers in `arith`, `memory`, `locals`, and `aggregates` share these helpers so operand
//! decoding and stack pops stay consistent across the interpreter.

use phx_bytecode::{Instruction, PrimitiveKind, ScalarValue};

use crate::VmErrorKind;
use crate::frame::Value;

/// Decodes the primitive-kind operand at `operand_index` on `inst`.
///
/// Reads `inst.operands[operand_index]` as a `u8` wire tag and maps it through
/// [`PrimitiveKind::from_u8`](phx_bytecode::PrimitiveKind::from_u8). Missing operands default to
/// `0` (the first enum discriminant). Used wherever an opcode carries an explicit wire type
/// separate from stack values (for example [`super::locals::exec_const`] operand 1, or arithmetic
/// opcodes that name their result kind).
///
/// # Errors
///
/// Returns [`VmErrorKind::InvalidConstPayload`] when the byte is not a known
/// [`PrimitiveKind`](phx_bytecode::PrimitiveKind) tag.
///
/// # Panics
///
/// Never panics.
pub(super) fn operand_prim_kind(
    inst: &Instruction,
    operand_index: usize,
) -> Result<PrimitiveKind, VmErrorKind> {
    let byte = inst.operands.get(operand_index).copied().unwrap_or(0) as u8;
    PrimitiveKind::from_u8(byte).ok_or(VmErrorKind::InvalidConstPayload)
}

/// Pops the stack top and returns it as a [`ScalarValue`].
///
/// Operand-stack cells are either [`Value::Scalar`] or [`Value::Agg`]. Handlers that expect a
/// primitive (arithmetic, pointer ops, indirect calls) use this helper instead of raw
/// [`Vec::pop`](std::vec::Vec::pop) so aggregate handles are rejected with a dedicated error
/// rather than propagating as the wrong shape downstream.
///
/// Stack: `[..., scalar] → [...]`.
///
/// # Errors
///
/// Returns [`VmErrorKind::StackUnderflow`] when the operand stack is empty.
/// Returns [`VmErrorKind::ExpectedScalar`] when the top cell is [`Value::Agg`].
///
/// # Panics
///
/// Never panics.
pub(super) fn pop_scalar(stack: &mut Vec<Value>) -> Result<ScalarValue, VmErrorKind> {
    match stack.pop().ok_or(VmErrorKind::StackUnderflow)? {
        Value::Scalar(v) => Ok(v),
        Value::Agg(_) => Err(VmErrorKind::ExpectedScalar),
    }
}

/// Converts an integer or bool scalar to `usize` for host-side indexing.
///
/// Widens all signed and unsigned integer storage variants and [`ScalarValue::Bool`] into `u64`,
/// then attempts [`usize::try_from`](usize::try_from). Used by aggregate opcodes
/// ([`super::aggregates`]) when a stack index or length must address a Rust `Vec` or slice.
/// Floating-point scalars and raw pointers are rejected because they are not valid indices.
///
/// # Errors
///
/// Returns [`VmErrorKind::ExpectedScalar`] for non-integer, non-bool scalars (floats, pointers).
/// Returns [`VmErrorKind::FieldOutOfRange`] when the widened value does not fit in `usize`.
///
/// # Panics
///
/// Never panics.
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

//! Stack arithmetic, comparison, bitwise, cast, and unary opcodes.
//!
//! Implements [`phx_bytecode::Opcode`] handlers that pop two scalars (or one for unary ops),
//! combine them according to the instruction's [`PrimitiveKind`](phx_bytecode::PrimitiveKind)
//! operand, and push the result. Binary ops follow the codegen stack convention: pop `b` then
//! `a`, push `op(a, b)`.
//!
//! Division and modulo trap on zero divisors; power is retained for opcode coverage but always
//! returns [`VmErrorKind::UnsupportedArithOp`]. Comparisons push a bool scalar. Operand decoding
//! and scalar pops are shared with [`super::util`].
//!
//! ## In this module
//!
//! | Handler family | Opcodes |
//! |----------------|---------|
//! | Arithmetic | `Add`, `Sub`, `Mul`, `Div`, `Mod`, `Pow` |
//! | Compare | `Eq`, `Ne`, `Lt`, `Le`, `Ge` |
//! | Unary / bitwise | `Neg`, `Not`, `BitNot`, `BitAnd`, `BitOr`, `BitXor`, `Shl`, `Shr` |
//! | Cast | `Cast` |

use phx_bytecode::{
    Instruction, PrimitiveKind, ScalarValue, mask_shift_amount, scalar_from_f64, scalar_from_i128,
    scalar_from_u128, scalar_to_f64, scalar_to_i128, scalar_to_u128,
};

use crate::VmErrorKind;
use crate::context::ExecutionContext;
use crate::frame::Value;

use super::util::{operand_prim_kind, pop_scalar};

/// [`Opcode::Add`](phx_bytecode::Opcode::Add) — binary add.
///
/// Stack: `[a, b] → [a + b]`. Operand: `prim_kind`.
///
/// # Errors
///
/// Returns [`VmErrorKind::StackUnderflow`], [`VmErrorKind::ExpectedScalar`], or arithmetic errors.
pub(super) fn exec_add(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    binop_arith(&mut ctx.stack, operand_prim_kind(inst, 0)?, ArithOp::Add)
}

/// Binary subtract.
pub(super) fn exec_sub(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    binop_arith(&mut ctx.stack, operand_prim_kind(inst, 0)?, ArithOp::Sub)
}

/// Binary multiply.
pub(super) fn exec_mul(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    binop_arith(&mut ctx.stack, operand_prim_kind(inst, 0)?, ArithOp::Mul)
}

/// Binary divide.
pub(super) fn exec_div(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    binop_arith(&mut ctx.stack, operand_prim_kind(inst, 0)?, ArithOp::Div)
}

/// Binary modulo.
pub(super) fn exec_mod(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    binop_arith(&mut ctx.stack, operand_prim_kind(inst, 0)?, ArithOp::Mod)
}

/// Binary power (cut from v0).
pub(super) fn exec_pow(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    binop_arith(&mut ctx.stack, operand_prim_kind(inst, 0)?, ArithOp::Pow)
}

/// Equality compare.
pub(super) fn exec_eq(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    binop_cmp(&mut ctx.stack, operand_prim_kind(inst, 0)?, CmpOp::Eq)
}

/// Less-than compare.
pub(super) fn exec_lt(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    binop_cmp(&mut ctx.stack, operand_prim_kind(inst, 0)?, CmpOp::Lt)
}

/// Not-equal compare.
pub(super) fn exec_ne(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    binop_cmp(&mut ctx.stack, operand_prim_kind(inst, 0)?, CmpOp::Ne)
}

/// Less-or-equal compare.
pub(super) fn exec_le(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    binop_cmp(&mut ctx.stack, operand_prim_kind(inst, 0)?, CmpOp::Le)
}

/// Greater-or-equal compare.
pub(super) fn exec_ge(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    binop_cmp(&mut ctx.stack, operand_prim_kind(inst, 0)?, CmpOp::Ge)
}

/// Numeric negation.
pub(super) fn exec_neg(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    let kind = operand_prim_kind(inst, 0)?;
    let v = pop_scalar(&mut ctx.stack)?;
    ctx.stack.push(Value::Scalar(neg_scalar(v, kind)));
    Ok(())
}

/// Logical not.
pub(super) fn exec_not(ctx: &mut ExecutionContext) -> Result<(), VmErrorKind> {
    let v = pop_scalar(&mut ctx.stack)?;
    ctx.stack
        .push(Value::Scalar(ScalarValue::Bool(!v.is_truthy())));
    Ok(())
}

/// Bitwise not.
pub(super) fn exec_bitnot(
    ctx: &mut ExecutionContext,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let kind = operand_prim_kind(inst, 0)?;
    let v = pop_scalar(&mut ctx.stack)?;
    ctx.stack.push(Value::Scalar(bitnot_scalar(v, kind)));
    Ok(())
}

/// Bitwise and.
pub(super) fn exec_bitand(
    ctx: &mut ExecutionContext,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    binop_bit(&mut ctx.stack, operand_prim_kind(inst, 0)?, BitOp::And)
}

/// Bitwise or.
pub(super) fn exec_bitor(
    ctx: &mut ExecutionContext,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    binop_bit(&mut ctx.stack, operand_prim_kind(inst, 0)?, BitOp::Or)
}

/// Bitwise xor.
pub(super) fn exec_bitxor(
    ctx: &mut ExecutionContext,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    binop_bit(&mut ctx.stack, operand_prim_kind(inst, 0)?, BitOp::Xor)
}

/// Shift left.
pub(super) fn exec_shl(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    binop_bit(&mut ctx.stack, operand_prim_kind(inst, 0)?, BitOp::Shl)
}

/// Shift right.
pub(super) fn exec_shr(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    binop_bit(&mut ctx.stack, operand_prim_kind(inst, 0)?, BitOp::Shr)
}

/// Explicit cast between primitive kinds.
pub(super) fn exec_cast(ctx: &mut ExecutionContext, inst: &Instruction) -> Result<(), VmErrorKind> {
    let from_byte = inst.operands.first().copied().unwrap_or(0) as u8;
    let to_byte = inst.operands.get(1).copied().unwrap_or(0) as u8;
    let from = PrimitiveKind::from_u8(from_byte).ok_or(VmErrorKind::InvalidConstPayload)?;
    let to = PrimitiveKind::from_u8(to_byte).ok_or(VmErrorKind::InvalidConstPayload)?;
    let v = pop_scalar(&mut ctx.stack)?;
    ctx.stack
        .push(Value::Scalar(PrimitiveKind::apply_cast(v, from, to)));
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

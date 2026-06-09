//! Operand stack effects for MVP opcodes (shared by verifier and codegen).

use crate::opcode::Opcode;

/// Stack simulation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackEffectError {
    /// Effect would pop below zero depth.
    Underflow,
    /// [`Opcode::Call`] requires callee arity.
    MissingCallArity,
    /// [`Opcode::MakeStruct`] / [`Opcode::MakeEnum`] / tuple/array require element count.
    MissingFieldCount,
}

/// Applies MVP stack effect for `opcode` to `depth`.
///
/// For [`Opcode::Call`], `call_arity` must be `Some(callee arity)`.
/// For [`Opcode::MakeStruct`] / [`Opcode::MakeEnum`] / [`Opcode::MakeTuple`] /
/// [`Opcode::MakeArray`], `field_count` is the number of values popped from the stack.
///
/// # Errors
///
/// Returns [`StackEffectError::Underflow`] when the effect would underflow `depth`.
pub fn apply_stack_effect(
    opcode: Opcode,
    depth: &mut u32,
    call_arity: Option<u16>,
    field_count: Option<u32>,
) -> Result<(), StackEffectError> {
    match opcode {
        Opcode::Const
        | Opcode::LoadLocal
        | Opcode::AddressOfLocal
        | Opcode::MakeStr
        | Opcode::MakeFnPtr => {
            *depth = depth.saturating_add(1);
        }
        Opcode::StoreLocal
        | Opcode::Add
        | Opcode::Sub
        | Opcode::Mul
        | Opcode::Div
        | Opcode::Mod
        | Opcode::Pow
        | Opcode::Eq
        | Opcode::Lt
        | Opcode::Ne
        | Opcode::Le
        | Opcode::Ge
        | Opcode::BitAnd
        | Opcode::BitOr
        | Opcode::BitXor
        | Opcode::Shl
        | Opcode::Shr
        | Opcode::JumpIfTrue
        | Opcode::JumpIfFalse
        | Opcode::Pop => {
            if *depth == 0 {
                return Err(StackEffectError::Underflow);
            }
            *depth -= 1;
        }
        Opcode::Index => {
            if *depth < 2 {
                return Err(StackEffectError::Underflow);
            }
            *depth -= 2;
            *depth += 1;
        }
        Opcode::Call => {
            let arity = u32::from(call_arity.ok_or(StackEffectError::MissingCallArity)?);
            if *depth < arity {
                return Err(StackEffectError::Underflow);
            }
            *depth -= arity;
            *depth += 1;
        }
        Opcode::CallIndirect => {
            let arity = u32::from(call_arity.ok_or(StackEffectError::MissingCallArity)?);
            if *depth < arity.saturating_add(1) {
                return Err(StackEffectError::Underflow);
            }
            *depth -= arity.saturating_add(1);
            *depth += 1;
        }
        Opcode::MakeStruct | Opcode::MakeEnum | Opcode::MakeTuple | Opcode::MakeArray => {
            let n = field_count.ok_or(StackEffectError::MissingFieldCount)?;
            if *depth < n {
                return Err(StackEffectError::Underflow);
            }
            *depth -= n;
            *depth += 1;
        }
        Opcode::GetField
        | Opcode::MatchTag
        | Opcode::MakeSlice
        | Opcode::StrAsSlice
        | Opcode::PtrLoad
        | Opcode::LoadAggViaLocalPtr
        | Opcode::Alloc => {
            if *depth == 0 {
                return Err(StackEffectError::Underflow);
            }
        }
        Opcode::SetField | Opcode::MakeSliceFromPtr => {
            if *depth < 2 {
                return Err(StackEffectError::Underflow);
            }
            *depth -= 1;
        }
        Opcode::Cast
        | Opcode::Neg
        | Opcode::Not
        | Opcode::BitNot
        | Opcode::Jump
        | Opcode::Return
        | Opcode::Trap => {}
        Opcode::PtrStore => {
            if *depth < 2 {
                return Err(StackEffectError::Underflow);
            }
            *depth -= 2;
        }
    }
    Ok(())
}

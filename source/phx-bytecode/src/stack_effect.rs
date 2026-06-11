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
        | Opcode::Pop => pop(depth, 1)?,
        Opcode::Index => {
            pop(depth, 2)?;
            *depth += 1;
        }
        Opcode::Call => {
            let arity = u32::from(call_arity.ok_or(StackEffectError::MissingCallArity)?);
            pop(depth, arity)?;
            *depth += 1;
        }
        Opcode::CallIndirect => {
            let arity = u32::from(call_arity.ok_or(StackEffectError::MissingCallArity)?);
            pop(depth, arity.saturating_add(1))?;
            *depth += 1;
        }
        Opcode::MakeStruct | Opcode::MakeEnum | Opcode::MakeTuple | Opcode::MakeArray => {
            let n = field_count.ok_or(StackEffectError::MissingFieldCount)?;
            pop(depth, n)?;
            *depth += 1;
        }
        Opcode::GetField
        | Opcode::MatchTag
        | Opcode::MakeSlice
        | Opcode::StrAsSlice
        | Opcode::PtrLoad
        | Opcode::LoadAggViaLocalPtr
        | Opcode::Alloc => require_depth(*depth, 1)?,
        Opcode::SetField | Opcode::MakeSliceFromPtr => pop_if_at_least(depth, 2, 1)?,
        Opcode::Cast
        | Opcode::Neg
        | Opcode::Not
        | Opcode::BitNot
        | Opcode::Jump
        | Opcode::Return
        | Opcode::Trap => {}
        Opcode::PtrStore | Opcode::Free => pop(depth, 2)?,
        Opcode::IndexStore => pop(depth, 3)?,
    }
    Ok(())
}

fn require_depth(depth: u32, needed: u32) -> Result<(), StackEffectError> {
    if depth < needed {
        Err(StackEffectError::Underflow)
    } else {
        Ok(())
    }
}

fn pop(depth: &mut u32, count: u32) -> Result<(), StackEffectError> {
    require_depth(*depth, count)?;
    *depth -= count;
    Ok(())
}

fn pop_if_at_least(
    depth: &mut u32,
    min_depth: u32,
    pop_count: u32,
) -> Result<(), StackEffectError> {
    require_depth(*depth, min_depth)?;
    *depth -= pop_count;
    Ok(())
}

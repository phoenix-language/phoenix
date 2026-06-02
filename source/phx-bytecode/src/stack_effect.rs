//! Operand stack effects for MVP opcodes (shared by verifier and codegen).

use crate::opcode::Opcode;

/// Stack simulation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackEffectError {
    /// Effect would pop below zero depth.
    Underflow,
    /// [`Opcode::Call`] requires callee arity.
    MissingCallArity,
    /// [`Opcode::MakeStruct`] / [`Opcode::MakeEnum`] require field/payload count.
    MissingFieldCount,
}

/// Applies MVP stack effect for `opcode` to `depth`.
///
/// For [`Opcode::Call`], `call_arity` must be `Some(callee arity)`.
/// For [`Opcode::MakeStruct`] / [`Opcode::MakeEnum`], `field_count` is the number of
/// values popped from the stack (fields or payload slots).
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
        Opcode::Const | Opcode::LoadLocal => {
            *depth = depth.saturating_add(1);
        }
        Opcode::StoreLocal
        | Opcode::Add
        | Opcode::Sub
        | Opcode::Mul
        | Opcode::Div
        | Opcode::Eq
        | Opcode::Lt
        | Opcode::JumpIfTrue
        | Opcode::JumpIfFalse
        | Opcode::Pop => {
            if *depth == 0 {
                return Err(StackEffectError::Underflow);
            }
            *depth -= 1;
        }
        Opcode::Call => {
            let arity = u32::from(call_arity.ok_or(StackEffectError::MissingCallArity)?);
            if *depth < arity {
                return Err(StackEffectError::Underflow);
            }
            *depth -= arity;
            *depth += 1;
        }
        Opcode::MakeStruct | Opcode::MakeEnum => {
            let n = field_count.ok_or(StackEffectError::MissingFieldCount)?;
            if *depth < n {
                return Err(StackEffectError::Underflow);
            }
            *depth -= n;
            *depth += 1;
        }
        Opcode::GetField | Opcode::MatchTag => {
            if *depth == 0 {
                return Err(StackEffectError::Underflow);
            }
        }
        Opcode::SetField => {
            if *depth < 2 {
                return Err(StackEffectError::Underflow);
            }
            *depth -= 1;
        }
        Opcode::Jump | Opcode::Return => {}
    }
    Ok(())
}

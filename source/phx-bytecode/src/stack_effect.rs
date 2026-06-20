//! Per-instruction operand stack deltas for MVP opcodes.
//!
//! Each [`Opcode`](crate::Opcode) variant documents its stack notation in
//! [`opcode.rs`](crate::opcode) (`[] → [value]`, `[a, b] → [sum]`, …). This module applies those
//! deltas to a running depth counter. It is **linear** — one opcode, one update — and does not
//! model control flow.
//!
//! ## Owning passes
//!
//! - **Verifier** — [`crate::stack_flow::analyze_stack_cfg`] calls [`apply_stack_effect`] at each
//!   instruction while simulating CFG paths and join points.
//! - **Codegen** — must emit instruction sequences whose per-opcode effects match this table
//!   (including arity operands for [`Opcode::Call`] and field counts for aggregate makers).
//!
//! ## Relationship to [`crate::stack_flow`]
//!
//! Short-circuit `&&` / `||` and other branching place successor blocks after unrelated paths, so
//! depth cannot be tracked by walking instructions in file order alone. [`stack_flow`](crate::stack_flow)
//! owns block-entry worklists and join checks; this module supplies the single-step delta each
//! simulation step applies.
//!
//! ## In this module
//!
//! - [`apply_stack_effect`] — update `depth` for one opcode (with optional arity / field count).
//! - [`StackEffectError`] — underflow or missing metadata for parameterized opcodes.

use crate::opcode::Opcode;

/// Stack simulation failure when applying a single opcode delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackEffectError {
    /// The effect would pop more values than `depth` currently holds.
    Underflow,
    /// [`Opcode::Call`] or [`Opcode::CallIndirect`] was applied without `call_arity`.
    MissingCallArity,
    /// [`Opcode::MakeStruct`], [`Opcode::MakeEnum`], [`Opcode::MakeTuple`], or
    /// [`Opcode::MakeArray`] was applied without `field_count`.
    MissingFieldCount,
}

/// Applies the MVP stack effect for `opcode` to `depth`.
///
/// Increments or decrements `depth` according to the stack notation on each [`Opcode`] variant.
/// Unary and nullary effects need no extra operands; parameterized opcodes require metadata:
///
/// | Opcode family | Extra argument |
/// | --- | --- |
/// | [`Opcode::Call`] | `call_arity` — callee parameter count (values popped) |
/// | [`Opcode::CallIndirect`] | `call_arity` — callee arity; one extra pop for the fn pointer |
/// | [`Opcode::MakeStruct`] / [`Opcode::MakeEnum`] / [`Opcode::MakeTuple`] / [`Opcode::MakeArray`] | `field_count` — values popped to build the aggregate |
///
/// Stack-neutral opcodes ([`Opcode::Jump`], [`Opcode::Return`], [`Opcode::Cast`], …) leave
/// `depth` unchanged. Depth-only checks ([`Opcode::GetField`], [`Opcode::Alloc`], …) require
/// `depth >= 1` but do not pop.
///
/// # Errors
///
/// - [`StackEffectError::Underflow`] — pop or depth check would exceed current `depth`.
/// - [`StackEffectError::MissingCallArity`] — [`Opcode::Call`] or [`Opcode::CallIndirect`] with
///   `call_arity: None`.
/// - [`StackEffectError::MissingFieldCount`] — aggregate maker with `field_count: None`.
///
/// # Panics
///
/// Never panics.
///
/// # Examples
///
/// Binary ops pop two operands and push one result:
///
/// ```
/// use phx_bytecode::{Opcode, apply_stack_effect};
///
/// let mut depth = 2;
/// apply_stack_effect(Opcode::Add, &mut depth, None, None).unwrap();
/// assert_eq!(depth, 1);
/// ```
///
/// [`Opcode::Call`] requires callee arity:
///
/// ```
/// use phx_bytecode::{Opcode, apply_stack_effect, StackEffectError};
///
/// let mut depth = 1;
/// assert_eq!(
///     apply_stack_effect(Opcode::Call, &mut depth, None, None),
///     Err(StackEffectError::MissingCallArity)
/// );
/// ```
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
        | Opcode::SliceLen
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

/// Returns [`StackEffectError::Underflow`] when `depth` is below `needed`.
fn require_depth(depth: u32, needed: u32) -> Result<(), StackEffectError> {
    if depth < needed {
        Err(StackEffectError::Underflow)
    } else {
        Ok(())
    }
}

/// Pops `count` operand-stack cells when `depth` is sufficient.
fn pop(depth: &mut u32, count: u32) -> Result<(), StackEffectError> {
    require_depth(*depth, count)?;
    *depth -= count;
    Ok(())
}

/// Requires at least `min_depth`, then pops `pop_count` (may leave depth above zero).
fn pop_if_at_least(
    depth: &mut u32,
    min_depth: u32,
    pop_count: u32,
) -> Result<(), StackEffectError> {
    require_depth(*depth, min_depth)?;
    *depth -= pop_count;
    Ok(())
}

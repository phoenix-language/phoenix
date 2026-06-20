//! Function-pointer materialization and indirect call dispatch.
//!
//! [`exec_make_fn_ptr`] wraps a Phoenix function id or foreign stub id in a tagged pointer via
//! [`fn_ptr_from_id`](phx_bytecode::fn_ptr_from_id). [`exec_call_indirect`] pops arguments and
//! the pointer, then dispatches to [`super::control::call_phoenix_with_args`] or
//! [`crate::foreign::dispatch_foreign`] according to the decoded target kind.
//!
//! Function pointers are [`ScalarValue::Ptr`] values tagged with [`PTR_FN_TAG`](phx_bytecode::PTR_FN_TAG).
//! `target_kind` `0` selects an in-module Phoenix [`FunctionRecord`](phx_bytecode::FunctionRecord);
//! `target_kind` `1` selects a foreign stub resolved by [`crate::foreign::dispatch_foreign`].

use phx_bytecode::{
    BytecodeModule, Instruction, ScalarValue, decode_fn_ptr, fn_ptr_from_id, is_fn_ptr,
};

use crate::VmErrorKind;
use crate::context::Machine;
use crate::foreign::dispatch_foreign;
use crate::frame::Value;

use super::control::call_phoenix_with_args;
use super::util::pop_scalar;

/// Materializes a tagged function pointer and pushes it onto the operand stack.
///
/// Operand 0 is `target_kind` (`0` = Phoenix `function_id`, `1` = foreign stub id). Operand 1 is
/// the target id. The result is encoded with [`fn_ptr_from_id`](phx_bytecode::fn_ptr_from_id) and
/// stored as [`ScalarValue::Ptr`].
///
/// Stack: `[...] → [..., fn_ptr]`.
///
/// Does not validate that `target_id` exists; [`exec_call_indirect`] and the bytecode verifier
/// catch invalid targets at call time.
///
/// # Panics
///
/// Never panics.
pub(super) fn exec_make_fn_ptr(ctx: &mut crate::context::ExecutionContext, inst: &Instruction) {
    let target_kind = inst.operands.first().copied().unwrap_or(0);
    let target_id = inst.operands.get(1).copied().unwrap_or(0);
    let ptr = fn_ptr_from_id(target_kind, target_id);
    ctx.stack.push(Value::Scalar(ScalarValue::Ptr(ptr)));
}

/// Calls through a function pointer on the operand stack.
///
/// Operand 0 is `expected_arity`, the number of argument cells below the pointer. Arguments are
/// popped in stack order (last parameter on top), then reversed so the first parameter is at
/// index `0` for [`call_phoenix_with_args`]. The function pointer is popped last and must be a
/// [`PTR_FN_TAG`](phx_bytecode::PTR_FN_TAG)-tagged [`ScalarValue::Ptr`].
///
/// Stack: `[..., arg0, ..., arg{n-1}, fn_ptr] → [...]` (plus callee frame effects).
///
/// Dispatch:
/// - `target_kind == 0` — direct Phoenix call via [`call_phoenix_with_args`].
/// - `target_kind == 1` — re-push arguments and invoke [`dispatch_foreign`].
///
/// # Errors
///
/// Returns [`VmErrorKind::StackUnderflow`] when the stack has fewer than `arity + 1` cells or a
/// pop fails mid-sequence.
/// Returns [`VmErrorKind::InvalidFnPtr`] when the popped value is not a tagged function pointer
/// or `target_kind` is not `0` or `1`.
/// Propagates errors from [`call_phoenix_with_args`] (unknown function, arity mismatch) and
/// [`dispatch_foreign`] (unknown stub, foreign trap).
///
/// # Panics
///
/// Never panics on malformed user bytecode; returns [`VmErrorKind`] instead.
pub(super) fn exec_call_indirect(
    machine: &mut Machine,
    module: &BytecodeModule,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let expected_arity = inst.operands.first().copied().unwrap_or(0);
    let arity = usize::try_from(expected_arity).map_err(|_| VmErrorKind::StackUnderflow)?;
    if machine.ctx.stack.len() < arity.saturating_add(1) {
        return Err(VmErrorKind::StackUnderflow);
    }
    let mut args = Vec::with_capacity(arity);
    for _ in 0..arity {
        args.push(machine.ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?);
    }
    args.reverse();
    let fn_ptr_val = pop_scalar(&mut machine.ctx.stack)?;
    let ScalarValue::Ptr(ptr) = fn_ptr_val else {
        return Err(VmErrorKind::InvalidFnPtr);
    };
    if !is_fn_ptr(ptr) {
        return Err(VmErrorKind::InvalidFnPtr);
    }
    let (target_kind, target_id) = decode_fn_ptr(ptr);
    match target_kind {
        0 => call_phoenix_with_args(machine, module, target_id, &args)?,
        1 => {
            for arg in &args {
                machine.ctx.stack.push(*arg);
            }
            dispatch_foreign(target_id, machine, module)?;
        }
        _ => return Err(VmErrorKind::InvalidFnPtr),
    }
    Ok(())
}

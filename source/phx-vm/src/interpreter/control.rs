//! Control-flow opcodes: branches, calls, returns, and stack discard.
//!
//! Updates the active [`crate::frame::Frame`] program counter for unconditional and conditional
//! jumps. [`exec_call`] and [`call_phoenix_with_args`] allocate a callee frame, bind arguments
//! from the stack (first parameter at the lower stack position), and leave execution at PC 0.
//! [`exec_return`] pops the current frame; when the call stack empties it yields
//! [`ReturnCapture`] for the integration test harness.
//!
//! Unit-returning callees push an empty tuple aggregate when the return stack is empty so
//! callers always receive one stack cell per [`phx_bytecode::Opcode::Call`].

use phx_bytecode::{BytecodeModule, FunctionRecord, Instruction};

use crate::VmErrorKind;
use crate::context::Machine;
use crate::frame::{Aggregate, Value};

use super::util::pop_scalar;

/// Optional early exit when the entry function returns.
pub(super) struct ReturnCapture {
    /// Local slots for `main` at return.
    pub main_locals: Vec<Value>,
    /// Aggregate arena at return.
    pub aggregates: Vec<Aggregate>,
    /// Stack value popped at top-level `Return`.
    pub return_value: Option<Value>,
}

/// Unconditional jump.
///
/// [`Opcode::Jump`](phx_bytecode::Opcode::Jump) — sets the active frame PC to operand 0.
pub(super) fn exec_jump(ctx: &mut crate::context::ExecutionContext, inst: &Instruction) {
    let target = inst.operands.first().copied().unwrap_or(0);
    if let Some(f) = ctx.frames.last_mut() {
        f.pc = target;
    }
}

/// Conditional jump when the popped bool is true.
pub(super) fn exec_jump_if_true(
    ctx: &mut crate::context::ExecutionContext,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let target = inst.operands.first().copied().unwrap_or(0);
    let cond = pop_scalar(&mut ctx.stack)?;
    if cond.is_truthy()
        && let Some(f) = ctx.frames.last_mut()
    {
        f.pc = target;
    }
    Ok(())
}

/// Conditional jump when the popped bool is false.
pub(super) fn exec_jump_if_false(
    ctx: &mut crate::context::ExecutionContext,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let target = inst.operands.first().copied().unwrap_or(0);
    let cond = pop_scalar(&mut ctx.stack)?;
    if !cond.is_truthy()
        && let Some(f) = ctx.frames.last_mut()
    {
        f.pc = target;
    }
    Ok(())
}

/// Direct call to a Phoenix function.
pub(super) fn exec_call(
    machine: &mut Machine,
    module: &BytecodeModule,
    inst: &Instruction,
) -> Result<(), VmErrorKind> {
    let callee_id = inst.operands.first().copied().unwrap_or(0);
    call_phoenix(machine, module, callee_id)
}

/// Placeholder pushed to the caller when a `()` callee returns with an empty stack.
///
/// Codegen treats every [`phx_bytecode::Opcode::Call`] as producing one stack cell; callers
/// discard unit results with [`phx_bytecode::Opcode::Pop`].
fn push_unit_return_value(machine: &mut Machine) {
    machine.ctx.stack.push(
        machine
            .runtime
            .push_aggregate(Aggregate::Tuple { elems: vec![] }),
    );
}

/// Returns from the current function, optionally finishing the run.
pub(super) fn exec_return(machine: &mut Machine) -> Option<ReturnCapture> {
    let return_value = machine.ctx.stack.pop();
    let main_locals = if machine.ctx.frames.len() == 1 {
        machine.ctx.frames.last().map(|f| f.locals.clone())
    } else {
        None
    };
    machine.pop_frame();
    if machine.ctx.frames.is_empty() {
        return Some(ReturnCapture {
            main_locals: main_locals.unwrap_or_default(),
            aggregates: std::mem::take(&mut machine.runtime.aggregates),
            return_value,
        });
    }
    if let Some(v) = return_value {
        machine.ctx.stack.push(v);
    } else {
        push_unit_return_value(machine);
    }
    None
}

/// Discards the stack top.
pub(super) fn exec_pop(ctx: &mut crate::context::ExecutionContext) -> Result<(), VmErrorKind> {
    let _ = ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?;
    Ok(())
}

/// Looks up a function record by id.
pub(super) fn find_function(module: &BytecodeModule, id: u32) -> Option<&FunctionRecord> {
    module
        .functions
        .functions
        .iter()
        .find(|f| f.function_id == id)
}

pub(super) fn call_phoenix(
    machine: &mut Machine,
    module: &BytecodeModule,
    callee_id: u32,
) -> Result<(), VmErrorKind> {
    let callee =
        find_function(module, callee_id).ok_or(VmErrorKind::InvalidFunctionId(callee_id))?;
    let arity = usize::from(callee.arity);
    if machine.ctx.stack.len() < arity {
        return Err(VmErrorKind::StackUnderflow);
    }
    let mut args = Vec::with_capacity(arity);
    for _ in 0..arity {
        args.push(machine.ctx.stack.pop().ok_or(VmErrorKind::StackUnderflow)?);
    }
    args.reverse();
    call_phoenix_with_args(machine, module, callee_id, &args)
}

pub(super) fn call_phoenix_with_args(
    machine: &mut Machine,
    module: &BytecodeModule,
    callee_id: u32,
    args: &[Value],
) -> Result<(), VmErrorKind> {
    let callee =
        find_function(module, callee_id).ok_or(VmErrorKind::InvalidFunctionId(callee_id))?;
    machine.push_frame(callee_id, callee.local_count, &module.local_layouts);
    let callee_frame = machine
        .ctx
        .frames
        .last_mut()
        .ok_or(VmErrorKind::InvalidFunctionId(callee_id))?;
    for (i, arg) in args.iter().enumerate() {
        if let Some(slot) = callee_frame.locals.get_mut(i) {
            *slot = *arg;
        }
    }
    Ok(())
}

pub(super) fn function_code<'a>(module: &'a BytecodeModule, rec: &FunctionRecord) -> &'a [u8] {
    let start = rec.code_offset as usize;
    let end = start.saturating_add(rec.code_len as usize);
    module.code.get(start..end).unwrap_or(&[])
}

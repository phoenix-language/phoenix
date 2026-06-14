//! Opcode interpreter for one PHX0 module.

mod aggregates;
mod arith;
mod control;
mod indirect;
mod locals;
mod memory;
mod util;

use phx_bytecode::{BytecodeModule, ENTRY_NONE, InstrError, Instruction, Opcode, VerifiedModule};

use crate::context::{DEFAULT_HEAP_CAP_BYTES, Machine};
use crate::error::{VmError, VmErrorKind};
use crate::frame::{Aggregate, Value};

/// Captured VM state when the entry function returns (integration tests only).
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct VmRunCapture {
    /// Local slots for `main` at return.
    pub main_locals: Vec<Value>,
    /// Aggregate arena at return (for struct/enum inspection).
    pub aggregates: Vec<Aggregate>,
    /// Stack value popped at top-level `Return`, if the stack was non-empty.
    pub return_value: Option<Value>,
}

impl VmRunCapture {
    /// Returns the value stored in `main` local slot `index`, if present.
    #[must_use]
    pub fn main_local(&self, index: usize) -> Option<Value> {
        self.main_locals.get(index).copied()
    }
}

/// Runs a verified `module` starting at `entry` until `main` returns.
///
/// # Errors
///
/// Returns [`VmError`] on runtime failure.
pub fn interpret(verified: VerifiedModule<'_>) -> Result<(), VmError> {
    run_captured(verified).map(|_| ())
}

/// Runs a verified `module` and returns `main` local slots captured at entry return.
///
/// Integration-test harness only; production callers use [`interpret`].
///
/// # Errors
///
/// Returns [`VmError`] on runtime failure.
#[doc(hidden)]
pub fn run_captured(verified: VerifiedModule<'_>) -> Result<VmRunCapture, VmError> {
    run_captured_with_heap_cap(verified, DEFAULT_HEAP_CAP_BYTES)
}

/// Runs a verified `module` with a custom heap byte cap (integration / stress tests only).
///
/// # Errors
///
/// Returns [`VmError`] on runtime failure or heap cap exhaustion.
#[doc(hidden)]
pub fn run_captured_with_heap_cap(
    verified: VerifiedModule<'_>,
    heap_cap: usize,
) -> Result<VmRunCapture, VmError> {
    run_captured_unverified_with_heap_cap(verified.module(), heap_cap)
}

/// Runs `module` without a verification token (mutation / VM error-path tests only).
///
/// # Errors
///
/// Returns [`VmError`] on invalid bytecode or unsupported opcodes.
#[doc(hidden)]
pub fn interpret_unverified(module: &BytecodeModule) -> Result<(), VmError> {
    run_captured_unverified(module).map(|_| ())
}

/// Runs `module` without a verification token (mutation / VM error-path tests only).
///
/// # Errors
///
/// Returns [`VmError`] on invalid bytecode or unsupported opcodes.
#[doc(hidden)]
pub fn run_captured_unverified(module: &BytecodeModule) -> Result<VmRunCapture, VmError> {
    run_captured_unverified_with_heap_cap(module, DEFAULT_HEAP_CAP_BYTES)
}

/// Runs `module` without a verification token and with a custom heap cap (tests only).
///
/// # Errors
///
/// Returns [`VmError`] on invalid bytecode, unsupported opcodes, or heap cap exhaustion.
#[doc(hidden)]
pub fn run_captured_unverified_with_heap_cap(
    module: &BytecodeModule,
    heap_cap: usize,
) -> Result<VmRunCapture, VmError> {
    let entry_id = module.header.entry_function_id;
    if entry_id == ENTRY_NONE {
        return Err(VmError::without_site(VmErrorKind::NoEntryPoint));
    }
    let entry = control::find_function(module, entry_id)
        .ok_or(VmErrorKind::MissingEntry)
        .map_err(VmError::without_site)?;
    if entry.arity != 0 {
        return Err(VmError::without_site(VmErrorKind::EntryArityNotZero));
    }

    let mut machine = Machine::with_heap_cap(heap_cap);
    machine.push_frame(entry_id, entry.local_count, &module.local_layouts);

    while let Some(frame) = machine.ctx.frames.last() {
        let fn_id = frame.function_id;
        let pc = frame.pc;

        let insn_result: Result<Option<VmRunCapture>, VmErrorKind> = (|| {
            let rec = control::find_function(module, fn_id)
                .ok_or(VmErrorKind::InvalidFunctionId(fn_id))?;
            let code = control::function_code(module, rec);
            let pc_usize = usize::try_from(pc).unwrap_or(0);
            if pc_usize >= code.len() {
                return Err(VmErrorKind::TruncatedCode);
            }

            let (inst, next_pc) = Instruction::decode_at(code, pc_usize).map_err(|e| match e {
                InstrError::Truncated | InstrError::TooManyOperands { .. } => {
                    VmErrorKind::TruncatedCode
                }
                InstrError::Opcode(phx_bytecode::OpcodeError::Unknown(op)) => {
                    VmErrorKind::UnsupportedOpcode(op)
                }
            })?;

            if let Some(frame) = machine.ctx.frames.last_mut() {
                frame.pc = u32::try_from(next_pc).unwrap_or(u32::MAX);
            }

            if let Some(capture) = dispatch_opcode(&mut machine, module, &inst)? {
                return Ok(Some(capture));
            }

            Ok(None)
        })();

        match insn_result {
            Ok(Some(capture)) => return Ok(capture),
            Ok(None) => {}
            Err(kind) => return Err(VmError::at(fn_id, pc, kind)),
        }
    }
    Ok(VmRunCapture {
        main_locals: Vec::new(),
        aggregates: std::mem::take(&mut machine.runtime.aggregates),
        return_value: None,
    })
}

fn dispatch_opcode(
    machine: &mut Machine,
    module: &BytecodeModule,
    inst: &Instruction,
) -> Result<Option<VmRunCapture>, VmErrorKind> {
    match inst.opcode {
        Opcode::Const => locals::exec_const(&mut machine.ctx, module, inst)?,
        Opcode::LoadLocal => locals::exec_load_local(&mut machine.ctx, inst)?,
        Opcode::StoreLocal => locals::exec_store_local(&mut machine.ctx, inst)?,
        Opcode::Add => arith::exec_add(&mut machine.ctx, inst)?,
        Opcode::Sub => arith::exec_sub(&mut machine.ctx, inst)?,
        Opcode::Mul => arith::exec_mul(&mut machine.ctx, inst)?,
        Opcode::Div => arith::exec_div(&mut machine.ctx, inst)?,
        Opcode::Mod => arith::exec_mod(&mut machine.ctx, inst)?,
        Opcode::Pow => arith::exec_pow(&mut machine.ctx, inst)?,
        Opcode::Eq => arith::exec_eq(&mut machine.ctx, inst)?,
        Opcode::Lt => arith::exec_lt(&mut machine.ctx, inst)?,
        Opcode::Ne => arith::exec_ne(&mut machine.ctx, inst)?,
        Opcode::Le => arith::exec_le(&mut machine.ctx, inst)?,
        Opcode::Ge => arith::exec_ge(&mut machine.ctx, inst)?,
        Opcode::Jump => control::exec_jump(&mut machine.ctx, inst),
        Opcode::JumpIfTrue => control::exec_jump_if_true(&mut machine.ctx, inst)?,
        Opcode::JumpIfFalse => control::exec_jump_if_false(&mut machine.ctx, inst)?,
        Opcode::Call => control::exec_call(machine, module, inst)?,
        Opcode::Return => {
            if let Some(cap) = control::exec_return(machine) {
                return Ok(Some(VmRunCapture {
                    main_locals: cap.main_locals,
                    aggregates: cap.aggregates,
                    return_value: cap.return_value,
                }));
            }
        }
        Opcode::Pop => control::exec_pop(&mut machine.ctx)?,
        Opcode::MakeStruct => {
            aggregates::exec_make_struct(&mut machine.ctx, &mut machine.runtime, inst)?;
        }
        Opcode::MakeEnum => {
            aggregates::exec_make_enum(&mut machine.ctx, &mut machine.runtime, inst)?;
        }
        Opcode::GetField => aggregates::exec_get_field(&mut machine.ctx, &machine.runtime, inst)?,
        Opcode::SetField => {
            aggregates::exec_set_field(&mut machine.ctx, &mut machine.runtime, inst)?;
        }
        Opcode::MatchTag => aggregates::exec_match_tag(&mut machine.ctx, &machine.runtime, inst)?,
        Opcode::Cast => arith::exec_cast(&mut machine.ctx, inst)?,
        Opcode::Neg => arith::exec_neg(&mut machine.ctx, inst)?,
        Opcode::Not => arith::exec_not(&mut machine.ctx)?,
        Opcode::BitNot => arith::exec_bitnot(&mut machine.ctx, inst)?,
        Opcode::BitAnd => arith::exec_bitand(&mut machine.ctx, inst)?,
        Opcode::BitOr => arith::exec_bitor(&mut machine.ctx, inst)?,
        Opcode::BitXor => arith::exec_bitxor(&mut machine.ctx, inst)?,
        Opcode::Shl => arith::exec_shl(&mut machine.ctx, inst)?,
        Opcode::Shr => arith::exec_shr(&mut machine.ctx, inst)?,
        Opcode::MakeTuple => {
            aggregates::exec_make_tuple(&mut machine.ctx, &mut machine.runtime, inst)?;
        }
        Opcode::MakeArray => {
            aggregates::exec_make_array(&mut machine.ctx, &mut machine.runtime, inst)?;
        }
        Opcode::Alloc => memory::exec_alloc(&mut machine.ctx, &mut machine.runtime)?,
        Opcode::Free => memory::exec_free(&mut machine.ctx, &mut machine.runtime)?,
        Opcode::PtrLoad => memory::exec_ptr_load(&mut machine.ctx, &machine.runtime, inst)?,
        Opcode::PtrStore => memory::exec_ptr_store(&mut machine.ctx, &mut machine.runtime, inst)?,
        Opcode::MakeSlice => {
            aggregates::exec_make_slice(&mut machine.ctx, &mut machine.runtime, inst)?;
        }
        Opcode::MakeSliceFromPtr => {
            aggregates::exec_make_slice_from_ptr(&mut machine.ctx, &mut machine.runtime, inst)?;
        }
        Opcode::AddressOfLocal => memory::exec_address_of_local(&mut machine.ctx, inst),
        Opcode::LoadAggViaLocalPtr => memory::exec_load_agg_via_local_ptr(&mut machine.ctx)?,
        Opcode::MakeStr => {
            aggregates::exec_make_str(&mut machine.ctx, &mut machine.runtime, module, inst)?;
        }
        Opcode::StrAsSlice => {
            aggregates::exec_str_as_slice(&mut machine.ctx, &mut machine.runtime)?;
        }
        Opcode::Index => aggregates::exec_index(&mut machine.ctx, &machine.runtime, module)?,
        Opcode::IndexStore => {
            aggregates::exec_index_store(&mut machine.ctx, &mut machine.runtime, inst)?;
        }
        Opcode::Trap => return Err(VmErrorKind::GivenMismatch),
        Opcode::MakeFnPtr => indirect::exec_make_fn_ptr(&mut machine.ctx, inst),
        Opcode::CallIndirect => indirect::exec_call_indirect(machine, module, inst)?,
    }
    Ok(None)
}

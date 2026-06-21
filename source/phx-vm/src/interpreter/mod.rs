//! Opcode dispatch loop for one [`BytecodeModule`](phx_bytecode::BytecodeModule) image.
//!
//! This module is the core of the Phoenix stack interpreter. A caller provides either a
//! [`VerifiedModule`] (production path) or a raw [`BytecodeModule`] reference (mutation tests
//! only); execution resolves the module entry function, drives a [`Machine`](crate::context::Machine)
//! until the call stack drains, and returns success or a site-attributed [`VmError`].
//!
//! ## Relationship to [`VerifiedModule`]
//!
//! [`VerifiedModule`] is an opaque proof token from [`phx_bytecode::verify`] that the image
//! passed static checks (valid jump targets, stack depth, section bounds). The interpreter
//! assumes those invariants on the verified path but still returns [`VmError`] on runtime faults
//! (division by zero, heap cap exhaustion, stack underflow). The `*_unverified` entry points skip
//! the token and are `#[doc(hidden)]` for VM mutation tests that inject malformed bytecode.
//!
//! Production callers reach this module through [`crate::run`] and [`crate::run_with_heap_cap`],
//! which delegate to [`interpret`] and discard captured state on success.
//!
//! ## Execution model
//!
//! 1. Resolve the module entry function ([`ENTRY_NONE`](phx_bytecode::ENTRY_NONE) is rejected).
//! 2. Build a [`Machine`](crate::context::Machine) with the requested heap cap
//!    ([`DEFAULT_HEAP_CAP_BYTES`](crate::context::DEFAULT_HEAP_CAP_BYTES) by default).
//! 3. Push the entry frame and loop: decode the next [`Instruction`] at the active frame PC,
//!    advance PC, then [`dispatch_opcode`] into a submodule handler.
//! 4. On [`Opcode::Return`](phx_bytecode::Opcode::Return) when only the entry frame remains,
//!    build [`VmRunCapture`] and exit. Nested returns pop callee frames and push the return value
//!    onto the caller's stack.
//!
//! [`dispatch_opcode`] is the single match over [`Opcode`]; submodule files implement one opcode
//! family each and mutate [`ExecutionContext`](crate::context::ExecutionContext) and/or
//! [`VmRuntime`](crate::context::VmRuntime). Stack convention matches codegen: binary ops pop `b`
//! then `a` and push `op(a, b)`.
//!
//! ## Submodules
//!
//! | Submodule | Responsibility |
//! | --- | --- |
//! | `aggregates` | Structs, enums, tuples, arrays, slices, strings, indexing |
//! | `arith` | Arithmetic, comparison, cast, bitwise, logical negation |
//! | `control` | Branches, calls, returns, stack pop |
//! | `indirect` | Function pointers and indirect call |
//! | `locals` | Constants and local load/store |
//! | `memory` | Linear heap alloc/free, pointer load/store, address-of |
//! | `util` | Shared stack helpers (scalar pop, operand decoding) |
//!
//! ## Entry points
//!
//! | Function | Audience |
//! | --- | --- |
//! | [`interpret`] | Production — run a [`VerifiedModule`] until entry returns |
//! | [`run_captured`] | `#[doc(hidden)]` — same as [`interpret`] but returns [`VmRunCapture`] |
//! | [`run_captured_with_heap_cap`] | `#[doc(hidden)]` — captured run with custom heap cap |
//! | [`interpret_unverified`] | `#[doc(hidden)]` — mutation / error-path tests without verify token |
//! | [`run_captured_unverified`] | `#[doc(hidden)]` — captured unverified run |
//! | [`run_captured_unverified_with_heap_cap`] | `#[doc(hidden)]` — unverified run + heap cap |

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

/// Snapshot of VM state when the module entry function returns.
///
/// Populated by [`run_captured`] and related `#[doc(hidden)]` helpers. Integration tests and
/// `phx run --dump-main` inspect [`Self::main_locals`] or [`Self::main_local`] to assert computed
/// results without parsing stdout. [`Self::aggregates`] holds the run-wide struct/enum/tuple arena
/// at return time. [`Self::return_value`] is set when the entry function leaves a value on the
/// operand stack at top-level [`Opcode::Return`](phx_bytecode::Opcode::Return).
///
/// Production callers use [`crate::run`] or [`interpret`] and do not need this struct.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct VmRunCapture {
    /// Local slots for the entry function at return (index matches bytecode local layout).
    pub main_locals: Vec<Value>,
    /// Aggregate arena at return (for struct/enum/tuple inspection in tests).
    pub aggregates: Vec<Aggregate>,
    /// Operand-stack value popped at top-level `Return`, if the stack was non-empty.
    pub return_value: Option<Value>,
}

impl VmRunCapture {
    /// Returns the value stored in the entry function's local slot `index`, if present.
    ///
    /// Used by `phx run --dump-main` and integration tests to assert computed results without
    /// parsing stdout. Out-of-range indices return `None` rather than panicking.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn main_local(&self, index: usize) -> Option<Value> {
        self.main_locals.get(index).copied()
    }
}

/// Runs a verified module from its entry function until entry returns.
///
/// Discards operand-stack and local state on success. For integration tests that need entry
/// locals or the optional return value, use the `#[doc(hidden)]` [`run_captured`] helper or
/// [`crate::run`] at the crate root (which calls this function).
///
/// # Errors
///
/// Returns [`VmError`] when execution fails ([`VmErrorKind::StackUnderflow`], heap cap exhaustion,
/// `Trap`, and other runtime faults). Site-attributed errors include `(function_id, pc)` when the
/// fault occurs during instruction dispatch.
///
/// # Panics
///
/// Never panics on verified bytecode or malformed user bytecode; returns [`VmError`] instead.
pub fn interpret(verified: VerifiedModule<'_>) -> Result<(), VmError> {
    run_captured(verified).map(|_| ())
}

/// Runs a verified module and returns captured entry state at return.
///
/// Integration-test harness only; production callers use [`interpret`] or [`crate::run`].
///
/// On success, inspect [`VmRunCapture::main_locals`] or [`VmRunCapture::main_local`] for entry
/// slot values, [`VmRunCapture::aggregates`] for struct/enum handles, and
/// [`VmRunCapture::return_value`] when the entry function leaves a value on the stack at
/// [`Opcode::Return`](phx_bytecode::Opcode::Return).
///
/// # Errors
///
/// Returns [`VmError`] on runtime failure with optional `(function_id, pc)` site attribution.
///
/// # Panics
///
/// Never panics on verified bytecode or malformed user bytecode; returns [`VmError`] instead.
#[doc(hidden)]
pub fn run_captured(verified: VerifiedModule<'_>) -> Result<VmRunCapture, VmError> {
    run_captured_with_heap_cap(verified, DEFAULT_HEAP_CAP_BYTES)
}

/// Runs a verified module with a custom linear-heap byte cap (integration / stress tests only).
///
/// Same execution loop as [`run_captured`]; only the [`VmRuntime`](crate::context::VmRuntime)
/// heap limit differs. When allocation would exceed `heap_cap`, returns
/// [`VmErrorKind::OutOfMemory`].
///
/// # Errors
///
/// Returns [`VmError`] on runtime failure or heap cap exhaustion.
///
/// # Panics
///
/// Never panics on verified bytecode or malformed user bytecode; returns [`VmError`] instead.
#[doc(hidden)]
pub fn run_captured_with_heap_cap(
    verified: VerifiedModule<'_>,
    heap_cap: usize,
) -> Result<VmRunCapture, VmError> {
    run_captured_unverified_with_heap_cap(verified.module(), heap_cap)
}

/// Runs a module without a verification token (mutation / VM error-path tests only).
///
/// Skips the [`VerifiedModule`] proof from [`phx_bytecode::verify`]. Do not use for production
/// execution of untrusted images. On success, discards captured state like [`interpret`].
///
/// # Errors
///
/// Returns [`VmError`] on invalid bytecode, unsupported opcodes, or runtime faults.
///
/// # Panics
///
/// Never panics on malformed user bytecode; returns [`VmError`] instead.
#[doc(hidden)]
pub fn interpret_unverified(module: &BytecodeModule) -> Result<(), VmError> {
    run_captured_unverified(module).map(|_| ())
}

/// Runs a module without a verification token and returns captured entry state (tests only).
///
/// Same as [`run_captured`] but accepts a raw [`BytecodeModule`] reference. See
/// [`interpret_unverified`] for when to use the unverified path.
///
/// # Errors
///
/// Returns [`VmError`] on invalid bytecode, unsupported opcodes, or runtime faults.
///
/// # Panics
///
/// Never panics on malformed user bytecode; returns [`VmError`] instead.
#[doc(hidden)]
pub fn run_captured_unverified(module: &BytecodeModule) -> Result<VmRunCapture, VmError> {
    run_captured_unverified_with_heap_cap(module, DEFAULT_HEAP_CAP_BYTES)
}

/// Runs a module without a verification token and with a custom heap cap (tests only).
///
/// Combines [`run_captured_unverified`] and [`run_captured_with_heap_cap`]. This is the
/// implementation root for all other entry points in this module.
///
/// # Errors
///
/// Returns [`VmError`] on invalid bytecode, unsupported opcodes, runtime faults, or heap cap
/// exhaustion ([`VmErrorKind::OutOfMemory`]).
///
/// # Panics
///
/// Never panics on malformed user bytecode; returns [`VmError`] instead.
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
        Opcode::SliceLen => {
            aggregates::exec_slice_len(&mut machine.ctx, &machine.runtime)?;
        }
        Opcode::Index => aggregates::exec_index(&mut machine.ctx, &machine.runtime, module)?,
        Opcode::IndexStore => {
            aggregates::exec_index_store(&mut machine.ctx, &mut machine.runtime, inst)?;
        }
        Opcode::Trap => return Err(VmErrorKind::GivenMismatch),
        Opcode::MakeFnPtr => indirect::exec_make_fn_ptr(&mut machine.ctx, inst),
        Opcode::CallIndirect => indirect::exec_call_indirect(machine, module, inst)?,
        Opcode::AwaitIo => {
            return Err(VmErrorKind::UnsupportedOpcode(inst.opcode.as_u8()));
        }
    }
    Ok(None)
}

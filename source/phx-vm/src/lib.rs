//! Phoenix VM — bytecode loader and stack interpreter (MVP).
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]
//!
//! Stack-machine interpreter for verified [`phx_bytecode::BytecodeModule`] images. Runtime contract:
//! `docs/design/features/vm-linear.md`.
//!
//! ## Entry points
//!
//! | API | Audience |
//! | --- | --- |
//! | [`run`] | Production — execute a [`VerifiedModule`] until `main` returns |
//! | [`run_with_heap_cap`] | Same as [`run`] with a custom linear-heap byte limit |
//! | [`run_unverified`] | `#[doc(hidden)]` — mutation / VM error-path tests only |
//!
//! Obtain a [`VerifiedModule`] with [`phx_bytecode::verify`] before calling [`run`]; the token
//! proves the verifier ran on the image.
//!
//! ## Test harness
//!
//! [`run_captured`] and related helpers are `#[doc(hidden)]` for integration tests that inspect
//! [`VmRunCapture::main_locals`], [`VmRunCapture::main_local`], or the optional stack
//! [`VmRunCapture::return_value`] after execution. Production callers should use [`run`].
//!
//! ## Errors
//!
//! Runtime failures return [`VmError`] with a [`VmErrorKind`] and optional coarse bytecode site
//! `(function_id, pc)` — the byte offset of the faulting instruction in that function's code.
//! Module-level failures omit the site. See [`VmError`] for `Display` formatting and attribution
//! rules.
//!
//! ## Re-exports
//!
//! - **Execution** — [`ExecutionContext`], [`Machine`], [`VmRuntime`], [`DEFAULT_HEAP_CAP_BYTES`]
//! - **Scheduler (PHX-sched-0/2/4)** — [`SingleThreadScheduler`], [`WorkerPool`],
//!   [`RunnableContext`], [`RunQueue`], [`ParkReason`], [`ContextState`], [`IoWaitRegistry`],
//!   [`IoHandle`]
//! - **Values** — [`Value`], [`Aggregate`]
//! - **Foreign stubs (Phase A)** — [`ForeignRegistry`], [`register_foreign_stub`],
//!   [`register_builtin_foreign_stubs`], [`PHOENIX_WRITE_STDOUT`]
//! - **Bytecode** — [`BytecodeModule`], [`VerifiedModule`] (from `phx_bytecode`)
//!
//! ## Stack convention
//!
//! Matches codegen: binary ops pop `b` then `a` and push `op(a, b)`; call pops arguments with the
//! first parameter taken from the lower stack position.

mod builtin_foreign;
mod context;
mod error;
mod foreign;
mod frame;
mod interpreter;
pub mod scheduler;

pub use builtin_foreign::{PHOENIX_WRITE_STDOUT, register_builtin_foreign_stubs};
pub use context::{DEFAULT_HEAP_CAP_BYTES, ExecutionContext, Machine, VmRuntime};
pub use error::{VmError, VmErrorKind};
pub use foreign::{
    ForeignRegistry, ForeignStubFn, clear_foreign_stubs, dispatch_foreign, dispatch_foreign_in,
    register_foreign_stub,
};
pub use frame::{Aggregate, Value};
pub use interpreter::{
    VmRunCapture, interpret_unverified, run_captured, run_captured_unverified,
    run_captured_unverified_with_heap_cap, run_captured_with_heap_cap,
};
pub use phx_bytecode::{BytecodeModule, VerifiedModule};
pub use scheduler::{
    ContextId, ContextState, IoHandle, IoWaitError, IoWaitRegistry, ParkReason, RunQueue,
    RunnableContext, RunningGuard, SchedulerError, SingleThreadScheduler, StepOutcome, WorkerPool,
    WorkerPoolStatus,
};

/// Runs a verified `module` from its entry function until `main` returns.
///
/// Discards operand-stack and local state on success. For integration tests that need `main` locals
/// or the optional return value, use the `#[doc(hidden)]` [`run_captured`] helper instead.
///
/// # Errors
///
/// Returns [`VmError`] when execution fails (stack underflow, heap cap, `Trap`, and other
/// [`VmErrorKind`] variants). Site-attributed errors include `(function_id, pc)` when known.
///
/// # Panics
///
/// Never panics on verified bytecode or malformed user bytecode; returns [`VmError`] instead.
pub fn run(verified: VerifiedModule<'_>) -> Result<(), VmError> {
    interpreter::interpret(verified)
}

/// Runs a verified `module` with a custom VM linear heap byte cap.
///
/// Uses [`run_captured_with_heap_cap`] internally and discards captured state. The default cap used
/// by [`run`] is [`DEFAULT_HEAP_CAP_BYTES`] (`64` MiB).
///
/// # Errors
///
/// Returns [`VmError`] when execution fails or the heap cap is exceeded
/// ([`VmErrorKind::OutOfMemory`]).
///
/// # Panics
///
/// Never panics on verified bytecode or malformed user bytecode; returns [`VmError`] instead.
pub fn run_with_heap_cap(verified: VerifiedModule<'_>, heap_cap: usize) -> Result<(), VmError> {
    run_captured_with_heap_cap(verified, heap_cap).map(|_| ())
}

/// Runs `module` without a verification token (mutation / VM error-path tests only).
///
/// Skips the [`VerifiedModule`] proof from [`phx_bytecode::verify`]. Do not use for production
/// execution of untrusted images.
///
/// # Errors
///
/// Returns [`VmError`] when execution fails.
///
/// # Panics
///
/// Never panics on malformed user bytecode; returns [`VmError`] instead.
#[doc(hidden)]
pub fn run_unverified(module: &BytecodeModule) -> Result<(), VmError> {
    interpreter::interpret_unverified(module)
}

#[cfg(test)]
mod tests {
    use phx_bytecode::{
        BytecodeModule, ConstEntry, ConstPool, ConstTag, FileHeader, FunctionRecord, FunctionTable,
        Instruction, Opcode, PcSpanTable, TypeTable, verify,
    };

    use super::{
        VmError, VmErrorKind, run, run_captured, run_captured_unverified_with_heap_cap,
        run_unverified,
    };

    trait TestInstructionEncode {
        fn test_encode(&self) -> Vec<u8>;
    }

    impl TestInstructionEncode for Instruction {
        fn test_encode(&self) -> Vec<u8> {
            match self.encode() {
                Ok(bytes) => bytes,
                Err(err) => panic!("encode {self:?}: {err:?}"),
            }
        }
    }

    fn code_offset(len: usize) -> u32 {
        match u32::try_from(len) {
            Ok(offset) => offset,
            Err(err) => panic!("code offset {len} exceeds u32::MAX: {err}"),
        }
    }

    fn verify_ok(module: &BytecodeModule) {
        if let Err(err) = verify(module) {
            panic!("verify should succeed: {err}");
        }
    }

    fn run_ok(module: &BytecodeModule) {
        let verified = match verify(module) {
            Ok(verified) => verified,
            Err(err) => panic!("verify should succeed: {err}"),
        };
        if let Err(err) = run(verified) {
            panic!("run should succeed: {err}");
        }
    }

    fn run_err(module: &BytecodeModule) -> VmError {
        let verified = match verify(module) {
            Ok(verified) => verified,
            Err(err) => panic!("verify should succeed: {err}"),
        };
        match run(verified) {
            Err(err) => err,
            Ok(()) => panic!("run should fail"),
        }
    }

    fn run_unverified_err(module: &BytecodeModule) -> VmError {
        match run_unverified(module) {
            Err(err) => err,
            Ok(()) => panic!("run_unverified should fail"),
        }
    }

    fn run_captured_err(module: &BytecodeModule) -> VmError {
        let verified = match verify(module) {
            Ok(verified) => verified,
            Err(err) => panic!("verify should succeed: {err}"),
        };
        match run_captured(verified) {
            Err(err) => err,
            Ok(_) => panic!("run_captured should fail"),
        }
    }

    fn run_captured_unverified_heap_err(module: &BytecodeModule, heap_cap: usize) -> VmError {
        match run_captured_unverified_with_heap_cap(module, heap_cap) {
            Err(err) => err,
            Ok(_) => panic!("run_captured_unverified_with_heap_cap should fail"),
        }
    }

    #[test]
    fn run_const_return() {
        let code = Instruction {
            opcode: Opcode::Return,
            operands: vec![],
        }
        .test_encode();

        let module = BytecodeModule {
            header: FileHeader::new(5, 0),
            constants: ConstPool::default(),
            types: TypeTable::default(),
            functions: FunctionTable {
                functions: vec![FunctionRecord {
                    function_id: 0,
                    name_symbol_id: 0,
                    arity: 0,
                    local_count: 0,
                    stack_max: 4,
                    flags: 0,
                    code_offset: 0,
                    code_len: u32::try_from(code.len()).unwrap_or(0),
                    return_type_id: 0,
                }],
            },
            code,
            local_layouts: phx_bytecode::LocalLayoutTable::default(),
            pc_spans: PcSpanTable::default(),
        };
        run_ok(&module);
    }

    fn minimal_module(code: Vec<u8>, local_count: u16, stack_max: u16) -> BytecodeModule {
        BytecodeModule {
            header: FileHeader::new(5, 0),
            constants: ConstPool::default(),
            types: TypeTable::default(),
            functions: FunctionTable {
                functions: vec![FunctionRecord {
                    function_id: 0,
                    name_symbol_id: 0,
                    arity: 0,
                    local_count,
                    stack_max,
                    flags: 0,
                    code_offset: 0,
                    code_len: u32::try_from(code.len()).unwrap_or(0),
                    return_type_id: 0,
                }],
            },
            code,
            local_layouts: phx_bytecode::LocalLayoutTable::default(),
            pc_spans: PcSpanTable::default(),
        }
    }

    #[test]
    fn run_unit_callee_call_pop_succeeds() {
        let unit_body: Vec<u8> = Instruction {
            opcode: Opcode::Return,
            operands: vec![],
        }
        .test_encode();
        let main_body: Vec<u8> = [
            Instruction {
                opcode: Opcode::Call,
                operands: vec![1],
            },
            Instruction {
                opcode: Opcode::Pop,
                operands: vec![],
            },
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            },
        ]
        .into_iter()
        .flat_map(|i| i.test_encode())
        .collect();
        let callee_off = code_offset(main_body.len());
        let mut code = main_body;
        code.extend(unit_body);
        let module = BytecodeModule {
            header: FileHeader::new(5, 0),
            constants: ConstPool::default(),
            types: TypeTable::default(),
            functions: FunctionTable {
                functions: vec![
                    FunctionRecord {
                        function_id: 0,
                        name_symbol_id: 0,
                        arity: 0,
                        local_count: 0,
                        stack_max: 4,
                        flags: 0,
                        code_offset: 0,
                        code_len: callee_off,
                        return_type_id: 0,
                    },
                    FunctionRecord {
                        function_id: 1,
                        name_symbol_id: 0,
                        arity: 0,
                        local_count: 0,
                        stack_max: 4,
                        flags: 0,
                        code_offset: callee_off,
                        code_len: code_offset(code.len()) - callee_off,
                        return_type_id: 0,
                    },
                ],
            },
            code,
            local_layouts: phx_bytecode::LocalLayoutTable::default(),
            pc_spans: PcSpanTable::default(),
        };
        run_ok(&module);
    }

    #[test]
    fn run_stack_underflow_returns_error() {
        let code = Instruction {
            opcode: Opcode::Add,
            operands: vec![],
        }
        .test_encode();
        let module = minimal_module(code, 0, 4);
        let err = run_unverified_err(&module);
        assert_eq!(err.kind, VmErrorKind::StackUnderflow);
        assert_eq!(err.function_id, Some(0));
        assert_eq!(err.pc, Some(0));
    }

    #[test]
    fn run_invalid_local_slot_returns_error() {
        let code = [
            Instruction {
                opcode: Opcode::LoadLocal,
                operands: vec![99, 2],
            },
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            },
        ]
        .into_iter()
        .flat_map(|i| i.test_encode())
        .collect::<Vec<_>>();
        let module = minimal_module(code, 1, 4);
        assert!(matches!(
            run_unverified_err(&module),
            VmError {
                kind: VmErrorKind::InvalidLocalSlot(99),
                function_id: Some(0),
                pc: Some(0),
                ..
            }
        ));
    }

    #[test]
    fn run_invalid_call_target_returns_error() {
        let code = [
            Instruction {
                opcode: Opcode::Call,
                operands: vec![99],
            },
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            },
        ]
        .into_iter()
        .flat_map(|i| i.test_encode())
        .collect::<Vec<_>>();
        let module = minimal_module(code, 0, 4);
        assert!(matches!(
            run_unverified_err(&module),
            VmError {
                kind: VmErrorKind::InvalidFunctionId(99),
                function_id: Some(0),
                pc: Some(0),
                ..
            }
        ));
    }

    #[test]
    fn run_trap_returns_given_mismatch() {
        let code = Instruction {
            opcode: Opcode::Trap,
            operands: vec![],
        }
        .test_encode();
        let module = minimal_module(code, 0, 4);
        verify_ok(&module);
        let err = run_err(&module);
        assert_eq!(err.kind, VmErrorKind::GivenMismatch);
        assert_eq!(err.function_id, Some(0));
        assert_eq!(err.pc, Some(0));
    }

    #[test]
    fn run_bytes_const_returns_unsupported() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0, 2],
            }
            .test_encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .test_encode(),
        );
        let module = BytecodeModule {
            header: FileHeader::new(5, 0),
            constants: ConstPool {
                entries: vec![ConstEntry {
                    tag: ConstTag::Bytes,
                    payload: vec![1, 2, 3],
                }],
            },
            types: TypeTable::default(),
            functions: FunctionTable {
                functions: vec![FunctionRecord {
                    function_id: 0,
                    name_symbol_id: 0,
                    arity: 0,
                    local_count: 0,
                    stack_max: 4,
                    flags: 0,
                    code_offset: 0,
                    code_len: u32::try_from(code.len()).unwrap_or(0),
                    return_type_id: 0,
                }],
            },
            code,
            local_layouts: phx_bytecode::LocalLayoutTable::default(),
            pc_spans: PcSpanTable::default(),
        };
        let err = run_unverified_err(&module);
        assert_eq!(err.kind, VmErrorKind::UnsupportedConst);
        assert_eq!(err.function_id, Some(0));
        assert_eq!(err.pc, Some(0));
    }

    #[test]
    fn run_fallthrough_without_return_returns_truncated_code() {
        let module = minimal_module(Vec::new(), 0, 4);
        let err = run_unverified_err(&module);
        assert_eq!(err.kind, VmErrorKind::TruncatedCode);
        assert_eq!(err.function_id, Some(0));
        assert_eq!(err.pc, Some(0));
    }

    #[test]
    fn alloc_loop_hits_heap_cap_with_out_of_memory() {
        use phx_bytecode::{ConstEntry, ConstPool, ConstTag, PrimitiveKind};

        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0, u32::from(PrimitiveKind::U32.as_u8())],
            }
            .test_encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Alloc,
                operands: vec![],
            }
            .test_encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Pop,
                operands: vec![],
            }
            .test_encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Jump,
                operands: vec![0],
            }
            .test_encode(),
        );

        let module = BytecodeModule {
            header: FileHeader::new(5, 0),
            constants: ConstPool {
                entries: vec![ConstEntry {
                    tag: ConstTag::UnsignedInt,
                    payload: 8u32.to_le_bytes().to_vec(),
                }],
            },
            types: TypeTable::default(),
            functions: FunctionTable {
                functions: vec![FunctionRecord {
                    function_id: 0,
                    name_symbol_id: 0,
                    arity: 0,
                    local_count: 0,
                    stack_max: 8,
                    flags: 0,
                    code_offset: 0,
                    code_len: u32::try_from(code.len()).unwrap_or(0),
                    return_type_id: 0,
                }],
            },
            code,
            local_layouts: phx_bytecode::LocalLayoutTable::default(),
            pc_spans: PcSpanTable::default(),
        };

        let err = run_captured_unverified_heap_err(&module, 32);
        assert_eq!(err.kind, VmErrorKind::OutOfMemory);
        assert!(err.function_id.is_some());
        assert!(err.pc.is_some());
    }

    #[test]
    fn ptr_load_after_free_returns_use_after_free() {
        use phx_bytecode::{
            ConstEntry, ConstPool, ConstTag, FunctionLocalLayout, LocalLayoutTable, LocalSlotKind,
            PrimitiveKind,
        };

        let u32_kind = u32::from(PrimitiveKind::U32.as_u8());
        let u8_kind = u32::from(PrimitiveKind::U8.as_u8());
        let u64_kind = u32::from(PrimitiveKind::U64.as_u8());

        let mut code = Vec::new();
        for inst in [
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0, u32_kind],
            },
            Instruction {
                opcode: Opcode::Alloc,
                operands: vec![],
            },
            Instruction {
                opcode: Opcode::StoreLocal,
                operands: vec![0, u64_kind],
            },
            Instruction {
                opcode: Opcode::LoadLocal,
                operands: vec![0, u64_kind],
            },
            Instruction {
                opcode: Opcode::Const,
                operands: vec![1, u8_kind],
            },
            Instruction {
                opcode: Opcode::PtrStore,
                operands: vec![u8_kind, 0],
            },
            Instruction {
                opcode: Opcode::LoadLocal,
                operands: vec![0, u64_kind],
            },
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0, u32_kind],
            },
            Instruction {
                opcode: Opcode::Free,
                operands: vec![],
            },
            Instruction {
                opcode: Opcode::LoadLocal,
                operands: vec![0, u64_kind],
            },
            Instruction {
                opcode: Opcode::PtrLoad,
                operands: vec![u8_kind, 0],
            },
        ] {
            code.extend(inst.test_encode());
        }

        let module = BytecodeModule {
            header: FileHeader::new(5, 0),
            constants: ConstPool {
                entries: vec![
                    ConstEntry {
                        tag: ConstTag::UnsignedInt,
                        payload: 4u32.to_le_bytes().to_vec(),
                    },
                    ConstEntry {
                        tag: ConstTag::UnsignedInt,
                        payload: 77u8.to_le_bytes().to_vec(),
                    },
                ],
            },
            types: TypeTable::default(),
            functions: FunctionTable {
                functions: vec![FunctionRecord {
                    function_id: 0,
                    name_symbol_id: 0,
                    arity: 0,
                    local_count: 1,
                    stack_max: 8,
                    flags: 0,
                    code_offset: 0,
                    code_len: u32::try_from(code.len()).unwrap_or(0),
                    return_type_id: 0,
                }],
            },
            code,
            local_layouts: LocalLayoutTable {
                layouts: vec![FunctionLocalLayout {
                    function_id: 0,
                    slots: vec![LocalSlotKind::primitive(PrimitiveKind::U64)],
                }],
            },
            pc_spans: PcSpanTable::default(),
        };

        verify_ok(&module);
        let err = run_captured_err(&module);
        assert_eq!(err.kind, VmErrorKind::UseAfterFree);
        assert!(err.function_id.is_some());
        assert!(err.pc.is_some());
    }
}

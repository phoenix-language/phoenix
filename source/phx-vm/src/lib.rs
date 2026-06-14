//! Phoenix VM — bytecode loader and stack interpreter (MVP).
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]
//!
//! Executes verified [`phx_bytecode::BytecodeModule`] images with a single-process stack machine.
//!
//! Production callers obtain a [`VerifiedModule`] via [`phx_bytecode::verify`] and pass it to
//! [`run`]. [`run_unverified`] is `#[doc(hidden)]` for mutation and VM error-path tests only.
//!
//! [`run_captured`] is `#[doc(hidden)]` and exists only for integration tests that inspect `main`
//! locals (via [`VmRunCapture::main_local`]) or the stack return value after execution.
//!
//! ## Stack convention
//!
//! Matches codegen: binary ops pop `b` then `a` and push `op(a, b)`; call pops arguments with the
//! first parameter taken from the lower stack position.

mod error;
mod foreign;
mod frame;
mod interpreter;

pub use error::VmError;
pub use foreign::{ForeignStubFn, clear_foreign_stubs, dispatch_foreign, register_foreign_stub};
pub use frame::{Aggregate, DEFAULT_HEAP_CAP_BYTES, Machine, Value};
pub use interpreter::{
    VmRunCapture, interpret_unverified, run_captured, run_captured_unverified,
    run_captured_unverified_with_heap_cap, run_captured_with_heap_cap,
};
pub use phx_bytecode::{BytecodeModule, VerifiedModule};

/// Runs a verified `module` from its entry function until `main` returns.
///
/// # Errors
///
/// Returns [`VmError`] when execution fails.
pub fn run(verified: VerifiedModule<'_>) -> Result<(), VmError> {
    interpreter::interpret(verified)
}

/// Runs `module` without a verification token (mutation / VM error-path tests only).
///
/// # Errors
///
/// Returns [`VmError`] when execution fails.
#[doc(hidden)]
pub fn run_unverified(module: &BytecodeModule) -> Result<(), VmError> {
    interpreter::interpret_unverified(module)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use phx_bytecode::{
        BytecodeModule, ConstEntry, ConstPool, ConstTag, FileHeader, FunctionRecord, FunctionTable,
        Instruction, Opcode, TypeTable, verify,
    };

    use super::{
        VmError, run, run_captured, run_captured_unverified_with_heap_cap, run_unverified,
    };

    #[test]
    fn run_const_return() {
        let code = Instruction {
            opcode: Opcode::Return,
            operands: vec![],
        }
        .encode()
        .expect("encode");

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
        };
        let verified = verify(&module).expect("verify");
        run(verified).expect("run");
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
        }
    }

    #[test]
    fn run_stack_underflow_returns_error() {
        let code = Instruction {
            opcode: Opcode::Add,
            operands: vec![],
        }
        .encode()
        .expect("encode");
        let module = minimal_module(code, 0, 4);
        assert_eq!(run_unverified(&module), Err(VmError::StackUnderflow));
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
        .flat_map(|i| i.encode().expect("encode"))
        .collect::<Vec<_>>();
        let module = minimal_module(code, 1, 4);
        assert!(matches!(
            run_unverified(&module),
            Err(VmError::InvalidLocalSlot(99))
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
        .flat_map(|i| i.encode().expect("encode"))
        .collect::<Vec<_>>();
        let module = minimal_module(code, 0, 4);
        assert!(matches!(
            run_unverified(&module),
            Err(VmError::InvalidFunctionId(99))
        ));
    }

    #[test]
    fn run_trap_returns_given_mismatch() {
        let code = Instruction {
            opcode: Opcode::Trap,
            operands: vec![],
        }
        .encode()
        .expect("encode");
        let module = minimal_module(code, 0, 4);
        let verified = verify(&module).expect("trap module verifies");
        assert_eq!(run(verified), Err(VmError::GivenMismatch));
    }

    #[test]
    fn run_bytes_const_returns_unsupported() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0, 2],
            }
            .encode()
            .expect("encode"),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode()
            .expect("encode"),
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
        };
        assert_eq!(run_unverified(&module), Err(VmError::UnsupportedConst));
    }

    #[test]
    fn run_fallthrough_without_return_returns_truncated_code() {
        let module = minimal_module(Vec::new(), 0, 4);
        assert_eq!(run_unverified(&module), Err(VmError::TruncatedCode));
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
            .encode()
            .expect("encode"),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Alloc,
                operands: vec![],
            }
            .encode()
            .expect("encode"),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Pop,
                operands: vec![],
            }
            .encode()
            .expect("encode"),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Jump,
                operands: vec![0],
            }
            .encode()
            .expect("encode"),
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
        };

        assert!(matches!(
            run_captured_unverified_with_heap_cap(&module, 32),
            Err(VmError::OutOfMemory)
        ));
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
            code.extend(inst.encode().expect("encode"));
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
        };

        let verified = verify(&module).expect("verify heap uaf bytecode");
        assert!(matches!(run_captured(verified), Err(VmError::UseAfterFree)));
    }
}

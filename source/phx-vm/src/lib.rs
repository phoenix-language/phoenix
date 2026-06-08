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
//! [`run`] is the production entry point. [`run_captured`] is `#[doc(hidden)]` and exists only for
//! integration tests that inspect `main` locals (via [`VmRunCapture::main_local`]) or the stack
//! return value after execution.
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
pub use frame::{Aggregate, Machine, Value};
pub use interpreter::{VmRunCapture, run_captured};
pub use phx_bytecode::BytecodeModule;

/// Runs `module` from its entry function until `main` returns.
///
/// # Errors
///
/// Returns [`VmError`] when execution fails.
pub fn run(module: &BytecodeModule) -> Result<(), VmError> {
    interpreter::interpret(module)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use phx_bytecode::{
        BytecodeModule, ConstEntry, ConstPool, ConstTag, FileHeader, FunctionRecord, FunctionTable,
        Instruction, Opcode, TypeTable,
    };

    use super::{VmError, run};

    #[test]
    fn run_const_return() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0, 2],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );

        let module = BytecodeModule {
            header: FileHeader::new(5, 0),
            constants: ConstPool {
                entries: vec![ConstEntry {
                    tag: ConstTag::SignedInt,
                    payload: 1i64.to_le_bytes().to_vec(),
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
        run(&module).expect("run");
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
        .encode();
        let module = minimal_module(code, 0, 4);
        assert_eq!(run(&module), Err(VmError::StackUnderflow));
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
        .flat_map(|i| i.encode())
        .collect::<Vec<_>>();
        let module = minimal_module(code, 1, 4);
        assert!(matches!(run(&module), Err(VmError::InvalidLocalSlot(99))));
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
        .flat_map(|i| i.encode())
        .collect::<Vec<_>>();
        let module = minimal_module(code, 0, 4);
        assert!(matches!(run(&module), Err(VmError::InvalidFunctionId(99))));
    }

    #[test]
    fn run_bytes_const_returns_unsupported() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0, 2],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
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
        assert_eq!(run(&module), Err(VmError::UnsupportedConst));
    }
}

//! Phoenix VM — bytecode loader and stack interpreter (MVP).
//!
//! Executes verified [`phx_bytecode::BytecodeModule`] images with a single-process stack machine.
//!
//! ## Stack convention
//!
//! Matches codegen: binary ops pop `b` then `a` and push `op(a, b)`; call pops arguments with the
//! first parameter taken from the lower stack position.

mod error;
mod frame;
mod interpreter;

pub use error::VmError;
pub use frame::{Aggregate, Value};
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
mod tests {
    use phx_bytecode::{
        BytecodeModule, ConstEntry, ConstPool, ConstTag, FileHeader, FunctionRecord, FunctionTable,
        Instruction, Opcode, TypeTable,
    };

    use super::run;

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
}

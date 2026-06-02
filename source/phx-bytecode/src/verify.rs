//! Bytecode verifier — header, sections, control flow, and stack depth.

use std::collections::{HashMap, HashSet};

use super::function::FunctionRecord;
use super::header::MAGIC;
use super::instr::{InstrError, Instruction};
use super::module::BytecodeModule;
use super::opcode::Opcode;
use super::stack_effect::{StackEffectError, apply_stack_effect};

/// Verifier failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// File too small for header.
    Truncated,
    /// Magic is not `PHX0`.
    BadMagic,
    /// Section table or payload extends past file end.
    SectionOutOfBounds,
    /// MVP flags field must be zero.
    NonZeroFlags,
    /// `entry_function_id` missing or has non-zero arity.
    InvalidEntryFunction,
    /// Function `code_offset`/`code_len` out of code section bounds.
    FunctionCodeOutOfBounds {
        /// Offending function id.
        function_id: u32,
    },
    /// Instruction stream could not be decoded.
    MalformedInstruction {
        /// Function containing the bad bytecode.
        function_id: u32,
        /// Offset within the function body.
        offset: u32,
    },
    /// Jump target is not an instruction boundary in the function.
    InvalidJumpTarget {
        /// Function containing the jump.
        function_id: u32,
        /// Offset of the jump instruction.
        offset: u32,
        /// Invalid target offset.
        target: u32,
    },
    /// Local slot index >= `local_count`.
    LocalIndexOutOfRange {
        /// Function containing the instruction.
        function_id: u32,
        /// Local slot operand.
        slot: u32,
    },
    /// Constant pool index out of range.
    InvalidConstIndex {
        /// Function containing the instruction.
        function_id: u32,
        /// Constant pool operand.
        index: u32,
    },
    /// Callee function id not in the module.
    InvalidCallTarget {
        /// Function containing the call.
        function_id: u32,
        /// Callee function id operand.
        callee: u32,
    },
    /// Stack depth would go negative while simulating a function body.
    StackUnderflow {
        /// Function containing the instruction.
        function_id: u32,
        /// Offset of the offending instruction.
        offset: u32,
    },
    /// Simulated max stack depth exceeds declared `stack_max`.
    StackExceedsMax {
        /// Function whose body exceeds its limit.
        function_id: u32,
        /// Observed max depth.
        observed: u32,
        /// Declared limit.
        limit: u16,
    },
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => write!(f, "truncated bytecode file"),
            Self::BadMagic => write!(f, "invalid magic (expected PHX0)"),
            Self::SectionOutOfBounds => write!(f, "section extends past file end"),
            Self::NonZeroFlags => write!(f, "non-zero header flags"),
            Self::InvalidEntryFunction => {
                write!(f, "entry function missing or has non-zero arity")
            }
            Self::FunctionCodeOutOfBounds { function_id } => {
                write!(f, "function {function_id} code range out of bounds")
            }
            Self::MalformedInstruction {
                function_id,
                offset,
            } => {
                write!(
                    f,
                    "malformed instruction in function {function_id} at offset {offset}"
                )
            }
            Self::InvalidJumpTarget {
                function_id,
                offset,
                target,
            } => write!(
                f,
                "invalid jump target {target} in function {function_id} at offset {offset}"
            ),
            Self::LocalIndexOutOfRange { function_id, slot } => {
                write!(
                    f,
                    "local slot {slot} out of range in function {function_id}"
                )
            }
            Self::InvalidConstIndex { function_id, index } => {
                write!(
                    f,
                    "constant index {index} out of range in function {function_id}"
                )
            }
            Self::InvalidCallTarget {
                function_id,
                callee,
            } => {
                write!(f, "call target {callee} invalid in function {function_id}")
            }
            Self::StackUnderflow {
                function_id,
                offset,
            } => {
                write!(
                    f,
                    "stack underflow in function {function_id} at offset {offset}"
                )
            }
            Self::StackExceedsMax {
                function_id,
                observed,
                limit,
            } => write!(
                f,
                "function {function_id} stack depth {observed} exceeds stack_max {limit}"
            ),
        }
    }
}

impl std::error::Error for VerifyError {}

/// Verifies `module` invariants required before execution (MVP subset).
///
/// # Errors
///
/// Returns [`VerifyError`] when layout, control flow, or stack limits are invalid.
pub fn verify(module: &BytecodeModule) -> Result<(), VerifyError> {
    verify_header_and_sections(module)?;
    verify_entry_function(module)?;
    let fn_arity = function_arity_map(module);
    let const_count = u32::try_from(module.constants.entries.len()).unwrap_or(u32::MAX);
    for func in &module.functions.functions {
        verify_function_body(func, module, &fn_arity, const_count)?;
    }
    Ok(())
}

fn verify_header_and_sections(module: &BytecodeModule) -> Result<(), VerifyError> {
    let bytes = module.encode();
    if bytes.len() < 24 {
        return Err(VerifyError::Truncated);
    }
    if bytes[0..4] != MAGIC {
        return Err(VerifyError::BadMagic);
    }
    if module.header.flags != 0 {
        return Err(VerifyError::NonZeroFlags);
    }
    let table_end = 24usize.saturating_add(
        usize::try_from(module.header.section_count)
            .unwrap_or(0)
            .saturating_mul(12),
    );
    if bytes.len() < table_end {
        return Err(VerifyError::Truncated);
    }
    for i in 0..module.header.section_count {
        let start = 24 + usize::try_from(i).unwrap_or(0).saturating_mul(12);
        let offset = u32::from_le_bytes([
            bytes[start + 4],
            bytes[start + 5],
            bytes[start + 6],
            bytes[start + 7],
        ]);
        let length = u32::from_le_bytes([
            bytes[start + 8],
            bytes[start + 9],
            bytes[start + 10],
            bytes[start + 11],
        ]);
        let end = u64::from(offset).saturating_add(u64::from(length));
        if end > bytes.len() as u64 {
            return Err(VerifyError::SectionOutOfBounds);
        }
    }
    Ok(())
}

fn verify_entry_function(module: &BytecodeModule) -> Result<(), VerifyError> {
    let entry_id = module.header.entry_function_id;
    let entry = module
        .functions
        .functions
        .iter()
        .find(|f| f.function_id == entry_id)
        .ok_or(VerifyError::InvalidEntryFunction)?;
    if entry.arity != 0 {
        return Err(VerifyError::InvalidEntryFunction);
    }
    Ok(())
}

fn function_arity_map(module: &BytecodeModule) -> HashMap<u32, u16> {
    module
        .functions
        .functions
        .iter()
        .map(|f| (f.function_id, f.arity))
        .collect()
}

fn function_code<'a>(module: &'a BytecodeModule, func: &FunctionRecord) -> Option<&'a [u8]> {
    let start = usize::try_from(func.code_offset).ok()?;
    let end = start.checked_add(usize::try_from(func.code_len).ok()?)?;
    module.code.get(start..end)
}

fn verify_function_body(
    func: &FunctionRecord,
    module: &BytecodeModule,
    fn_arity: &HashMap<u32, u16>,
    const_count: u32,
) -> Result<(), VerifyError> {
    let code = function_code(module, func).ok_or(VerifyError::FunctionCodeOutOfBounds {
        function_id: func.function_id,
    })?;
    if code.len() != usize::try_from(func.code_len).unwrap_or(usize::MAX) {
        return Err(VerifyError::FunctionCodeOutOfBounds {
            function_id: func.function_id,
        });
    }

    let mut inst_starts = HashSet::new();
    let mut instructions = Vec::new();
    let mut offset = 0usize;
    while offset < code.len() {
        let rel = u32::try_from(offset).unwrap_or(u32::MAX);
        inst_starts.insert(rel);
        match Instruction::decode_at(code, offset) {
            Ok((inst, next)) => {
                instructions.push((rel, inst));
                offset = next;
            }
            Err(InstrError::Truncated | InstrError::Opcode(_)) => {
                return Err(VerifyError::MalformedInstruction {
                    function_id: func.function_id,
                    offset: rel,
                });
            }
        }
    }

    let code_len = func.code_len;
    let mut depth = 0u32;
    let mut max_depth = 0u32;

    for (rel, inst) in &instructions {
        verify_operands(
            func,
            *rel,
            &inst,
            &inst_starts,
            code_len,
            fn_arity,
            const_count,
        )?;

        let call_arity = if inst.opcode == Opcode::Call {
            let callee = inst.operands.first().copied().unwrap_or(0);
            Some(
                *fn_arity
                    .get(&callee)
                    .ok_or(VerifyError::InvalidCallTarget {
                        function_id: func.function_id,
                        callee,
                    })?,
            )
        } else {
            None
        };

        let field_count = match inst.opcode {
            Opcode::MakeStruct => Some(inst.operands.get(1).copied().unwrap_or(0)),
            Opcode::MakeEnum => Some(inst.operands.get(2).copied().unwrap_or(0)),
            Opcode::MakeTuple | Opcode::MakeArray => inst.operands.first().copied(),
            _ => None,
        };

        apply_stack_effect(inst.opcode, &mut depth, call_arity, field_count).map_err(
            |e| match e {
                StackEffectError::Underflow
                | StackEffectError::MissingCallArity
                | StackEffectError::MissingFieldCount => VerifyError::StackUnderflow {
                    function_id: func.function_id,
                    offset: *rel,
                },
            },
        )?;
        max_depth = max_depth.max(depth);
    }

    if max_depth > u32::from(func.stack_max) {
        return Err(VerifyError::StackExceedsMax {
            function_id: func.function_id,
            observed: max_depth,
            limit: func.stack_max,
        });
    }

    Ok(())
}

fn verify_operands(
    func: &FunctionRecord,
    offset: u32,
    inst: &Instruction,
    inst_starts: &HashSet<u32>,
    code_len: u32,
    fn_arity: &HashMap<u32, u16>,
    const_count: u32,
) -> Result<(), VerifyError> {
    let function_id = func.function_id;
    match inst.opcode {
        Opcode::Const => {
            let index = inst.operands.first().copied().unwrap_or(0);
            if index >= const_count {
                return Err(VerifyError::InvalidConstIndex { function_id, index });
            }
        }
        Opcode::LoadLocal | Opcode::StoreLocal => {
            let slot = inst.operands.first().copied().unwrap_or(0);
            if slot >= u32::from(func.local_count) {
                return Err(VerifyError::LocalIndexOutOfRange { function_id, slot });
            }
        }
        Opcode::Call => {
            let callee = inst.operands.first().copied().unwrap_or(0);
            if !fn_arity.contains_key(&callee) {
                return Err(VerifyError::InvalidCallTarget {
                    function_id,
                    callee,
                });
            }
        }
        Opcode::Jump | Opcode::JumpIfTrue | Opcode::JumpIfFalse => {
            let target = inst.operands.first().copied().unwrap_or(0);
            if target >= code_len || !inst_starts.contains(&target) {
                return Err(VerifyError::InvalidJumpTarget {
                    function_id,
                    offset,
                    target,
                });
            }
        }
        Opcode::Add
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
        | Opcode::Neg
        | Opcode::Not
        | Opcode::BitNot
        | Opcode::Index
        | Opcode::Pop
        | Opcode::Return => {
            if !inst.operands.is_empty() {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
        }
        Opcode::Cast => {
            if inst.operands.len() != 2 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
        }
        Opcode::MakeTuple | Opcode::MakeArray => {
            if inst.operands.len() != 1 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
        }
        Opcode::Trap => {
            if inst.operands.len() != 1 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
        }
        Opcode::MakeStruct => {
            if inst.operands.len() != 2 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
        }
        Opcode::MakeEnum => {
            if inst.operands.len() != 3 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
        }
        Opcode::GetField | Opcode::SetField | Opcode::MatchTag => {
            if inst.operands.len() != 2 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
        }
        Opcode::Alloc => {
            if inst.operands.len() != 1 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
        }
        Opcode::PtrLoad | Opcode::PtrStore => {
            if inst.operands.len() != 2 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ConstEntry, ConstPool, ConstTag, FileHeader, FunctionRecord, FunctionTable, Instruction,
        Opcode, TypeTable,
    };

    fn minimal_module(code: Vec<u8>, stack_max: u16, entry_arity: u16) -> BytecodeModule {
        BytecodeModule {
            header: FileHeader::new(4, 0),
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
                    arity: entry_arity,
                    local_count: 0,
                    stack_max,
                    flags: 0,
                    code_offset: 0,
                    code_len: u32::try_from(code.len()).unwrap_or(0),
                    return_type_id: 0,
                }],
            },
            code,
        }
    }

    fn const_return_code() -> Vec<u8> {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0],
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
        code
    }

    #[test]
    fn valid_const_return_passes() {
        let module = minimal_module(const_return_code(), 4, 0);
        verify(&module).expect("verify");
    }

    #[test]
    fn reject_invalid_entry_arity() {
        let module = minimal_module(const_return_code(), 4, 1);
        let err = verify(&module).unwrap_err();
        assert_eq!(err, VerifyError::InvalidEntryFunction);
    }

    #[test]
    fn reject_invalid_jump_target() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::JumpIfTrue,
                operands: vec![1],
            }
            .encode(),
        );
        let module = minimal_module(code, 4, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::InvalidJumpTarget { function_id: 0, .. }
        ));
    }

    #[test]
    fn reject_local_out_of_range() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::LoadLocal,
                operands: vec![0],
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
        let module = minimal_module(code, 4, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::LocalIndexOutOfRange { slot: 0, .. }
        ));
    }

    #[test]
    fn reject_stack_exceeds_max() {
        let module = minimal_module(const_return_code(), 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::StackExceedsMax {
                observed: 1,
                limit: 0,
                ..
            }
        ));
    }
}

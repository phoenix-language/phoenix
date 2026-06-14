//! Shared bytecode test helpers for integration-style tests.
#![allow(clippy::cast_lossless, clippy::expect_used, clippy::unwrap_used)]

use phx_bytecode::{
    BytecodeModule, ConstEntry, ConstPool, ConstTag, FileHeader, FunctionRecord, FunctionTable,
    Instruction, LocalLayoutTable, Opcode, PrimitiveKind, TypeTable,
};

/// Minimal single-function module for verifier and mutation tests.
#[must_use]
pub fn minimal_module(
    code: Vec<u8>,
    stack_max: u16,
    entry_arity: u16,
    return_type_id: u32,
) -> BytecodeModule {
    BytecodeModule {
        header: FileHeader::new(5, 0),
        constants: ConstPool {
            entries: vec![ConstEntry {
                tag: ConstTag::SignedInt,
                payload: 1i32.to_le_bytes().to_vec(),
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
                return_type_id,
            }],
        },
        code,
        local_layouts: LocalLayoutTable::default(),
    }
}

/// `Const` + `Return` for function 0.
#[must_use]
pub fn const_return_code() -> Vec<u8> {
    let mut code = Vec::new();
    code.extend(
        Instruction {
            opcode: Opcode::Const,
            operands: vec![0, u32::from(PrimitiveKind::S32.as_u8())],
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

/// Valid baseline module used as the mutation source.
#[must_use]
pub fn valid_const_return_module() -> BytecodeModule {
    minimal_module(const_return_code(), 4, 0, 1)
}

/// `Const 4u32` → `Alloc` → store ptr → `PtrStore 77u8` → `PtrLoad` → store u8 → `Return`.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn heap_alloc_roundtrip_module() -> BytecodeModule {
    use phx_bytecode::{FunctionLocalLayout, LocalSlotKind};

    let mut code = Vec::new();
    code.extend(
        Instruction {
            opcode: Opcode::Const,
            operands: vec![0, u32::from(PrimitiveKind::U32.as_u8())],
        }
        .encode(),
    );
    code.extend(
        Instruction {
            opcode: Opcode::Alloc,
            operands: vec![],
        }
        .encode(),
    );
    code.extend(
        Instruction {
            opcode: Opcode::StoreLocal,
            operands: vec![0, u32::from(PrimitiveKind::U64.as_u8())],
        }
        .encode(),
    );
    code.extend(
        Instruction {
            opcode: Opcode::LoadLocal,
            operands: vec![0, u32::from(PrimitiveKind::U64.as_u8())],
        }
        .encode(),
    );
    code.extend(
        Instruction {
            opcode: Opcode::Const,
            operands: vec![1, u32::from(PrimitiveKind::U8.as_u8())],
        }
        .encode(),
    );
    code.extend(
        Instruction {
            opcode: Opcode::PtrStore,
            operands: vec![u32::from(PrimitiveKind::U8.as_u8()), 0],
        }
        .encode(),
    );
    code.extend(
        Instruction {
            opcode: Opcode::LoadLocal,
            operands: vec![0, u32::from(PrimitiveKind::U64.as_u8())],
        }
        .encode(),
    );
    code.extend(
        Instruction {
            opcode: Opcode::PtrLoad,
            operands: vec![u32::from(PrimitiveKind::U8.as_u8()), 0],
        }
        .encode(),
    );
    code.extend(
        Instruction {
            opcode: Opcode::StoreLocal,
            operands: vec![1, u32::from(PrimitiveKind::U8.as_u8())],
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

    BytecodeModule {
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
                local_count: 2,
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
                slots: vec![
                    LocalSlotKind::primitive(PrimitiveKind::U64),
                    LocalSlotKind::primitive(PrimitiveKind::U8),
                ],
            }],
        },
    }
}

//! Shared bytecode test helpers for integration-style tests.
#![allow(
    clippy::cast_lossless,
    dead_code,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::missing_panics_doc
)]

use phx_bytecode::{
    BytecodeModule, ConstEntry, ConstPool, ConstTag, FileHeader, FunctionRecord, FunctionTable,
    Instruction, LocalLayoutTable, Opcode, PcSpanTable, PrimitiveKind, TypeTable,
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
        pc_spans: PcSpanTable::default(),
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
    code
}

/// Valid baseline module used as the mutation source.
#[must_use]
pub fn valid_const_return_module() -> BytecodeModule {
    minimal_module(const_return_code(), 4, 0, 1)
}

/// If/else merge with divergent stack depths: then-branch leaves 1 value, else leaves 0.
///
/// Returns `(code, merge_offset)` where `merge_offset` is the join point with mismatched depth.
#[must_use]
pub fn join_depth_mismatch_code() -> (Vec<u8>, u32) {
    let mut code = Vec::new();
    code.extend(
        Instruction {
            opcode: Opcode::Const,
            operands: vec![0, u32::from(PrimitiveKind::Bool.as_u8())],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::JumpIfTrue,
            operands: vec![0],
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
    let then_off = u32::try_from(code.len()).unwrap_or(0);
    code.extend(
        Instruction {
            opcode: Opcode::Const,
            operands: vec![1, u32::from(PrimitiveKind::S32.as_u8())],
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
    let merge_off = u32::try_from(code.len()).unwrap_or(0);
    code.extend(
        Instruction {
            opcode: Opcode::Return,
            operands: vec![],
        }
        .encode()
        .expect("encode"),
    );

    let patch_operand = |code: &mut Vec<u8>, inst_offset: usize, target: u32| {
        let start = inst_offset + 2;
        code[start..start + 4].copy_from_slice(&target.to_le_bytes());
    };
    patch_operand(&mut code, 10, then_off);
    patch_operand(&mut code, 16, merge_off);
    patch_operand(
        &mut code,
        usize::try_from(then_off).unwrap_or(0) + 10,
        merge_off,
    );
    (code, merge_off)
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
            opcode: Opcode::StoreLocal,
            operands: vec![0, u32::from(PrimitiveKind::U64.as_u8())],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::LoadLocal,
            operands: vec![0, u32::from(PrimitiveKind::U64.as_u8())],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::Const,
            operands: vec![1, u32::from(PrimitiveKind::U8.as_u8())],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::PtrStore,
            operands: vec![u32::from(PrimitiveKind::U8.as_u8()), 0],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::LoadLocal,
            operands: vec![0, u32::from(PrimitiveKind::U64.as_u8())],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::PtrLoad,
            operands: vec![u32::from(PrimitiveKind::U8.as_u8()), 0],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::StoreLocal,
            operands: vec![1, u32::from(PrimitiveKind::U8.as_u8())],
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
        pc_spans: PcSpanTable::default(),
    }
}

/// `Alloc` → `MakeSliceFromPtr` (s32) → `IndexStore` 1 → `Index` load → local s32 slot.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn heap_s32_slice_index_roundtrip_module() -> BytecodeModule {
    use phx_bytecode::{FunctionLocalLayout, LocalSlotKind, SLOT_KIND_AGG};

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
            opcode: Opcode::Const,
            operands: vec![1, u32::from(PrimitiveKind::U32.as_u8())],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::MakeSliceFromPtr,
            operands: vec![u32::from(PrimitiveKind::S32.as_u8())],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::StoreLocal,
            operands: vec![0, u32::from(SLOT_KIND_AGG)],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::LoadLocal,
            operands: vec![0, u32::from(SLOT_KIND_AGG)],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::Const,
            operands: vec![2, u32::from(PrimitiveKind::U32.as_u8())],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::Const,
            operands: vec![3, u32::from(PrimitiveKind::S32.as_u8())],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::IndexStore,
            operands: vec![u32::from(PrimitiveKind::S32.as_u8()), 1],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::LoadLocal,
            operands: vec![0, u32::from(SLOT_KIND_AGG)],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::Const,
            operands: vec![2, u32::from(PrimitiveKind::U32.as_u8())],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::Index,
            operands: vec![],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::StoreLocal,
            operands: vec![1, u32::from(PrimitiveKind::S32.as_u8())],
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

    BytecodeModule {
        header: FileHeader::new(5, 0),
        constants: ConstPool {
            entries: vec![
                ConstEntry {
                    tag: ConstTag::UnsignedInt,
                    payload: 16u32.to_le_bytes().to_vec(),
                },
                ConstEntry {
                    tag: ConstTag::UnsignedInt,
                    payload: 4u32.to_le_bytes().to_vec(),
                },
                ConstEntry {
                    tag: ConstTag::UnsignedInt,
                    payload: 0u32.to_le_bytes().to_vec(),
                },
                ConstEntry {
                    tag: ConstTag::SignedInt,
                    payload: 1i32.to_le_bytes().to_vec(),
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
                    LocalSlotKind::aggregate(),
                    LocalSlotKind::primitive(PrimitiveKind::S32),
                ],
            }],
        },
        pc_spans: PcSpanTable::default(),
    }
}

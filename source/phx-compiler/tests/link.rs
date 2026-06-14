//! PHX0 linker: global `function_id` stability and `Call` operands after merge.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_bytecode::ENTRY_NONE;
use phx_bytecode::{
    BytecodeModule, ConstEntry, ConstPool, ConstTag, FileHeader, FunctionRecord, FunctionTable,
    Instruction, LocalLayoutTable, Opcode, PrimitiveKind, TypeKind, TypeRecord, TypeTable, verify,
};
use phx_compiler::{LinkInput, link_modules};

fn module_with_fn(fn_id: u32, code: Vec<u8>, stack_max: u16) -> BytecodeModule {
    module_with_tables(
        fn_id,
        code,
        stack_max,
        ConstPool::default(),
        TypeTable::default(),
    )
}

fn module_with_tables(
    fn_id: u32,
    code: Vec<u8>,
    stack_max: u16,
    constants: ConstPool,
    types: TypeTable,
) -> BytecodeModule {
    let code_len = u32::try_from(code.len()).unwrap_or(0);
    BytecodeModule {
        header: FileHeader::new(5, ENTRY_NONE),
        constants,
        types,
        functions: FunctionTable {
            functions: vec![FunctionRecord {
                function_id: fn_id,
                name_symbol_id: 0,
                arity: 0,
                local_count: 0,
                stack_max,
                flags: 0,
                code_offset: 0,
                code_len,
                return_type_id: 0,
            }],
        },
        code,
        local_layouts: LocalLayoutTable::default(),
    }
}

fn return_only() -> Vec<u8> {
    Instruction {
        opcode: Opcode::Return,
        operands: vec![],
    }
    .encode()
}

fn call_then_return(callee: u32) -> Vec<u8> {
    let mut code = Instruction {
        opcode: Opcode::Call,
        operands: vec![callee],
    }
    .encode();
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
fn link_preserves_call_function_id_operands() {
    let object_a = module_with_fn(0, return_only(), 0);
    let object_b = module_with_fn(1, call_then_return(0), 1);

    let linked = link_modules(
        &[
            LinkInput {
                logical_path: "a::callee".to_owned(),
                module: object_a,
            },
            LinkInput {
                logical_path: "b::caller".to_owned(),
                module: object_b,
            },
        ],
        1,
    )
    .expect("link");

    let ids: Vec<_> = linked
        .functions
        .functions
        .iter()
        .map(|f| f.function_id)
        .collect();
    assert!(ids.contains(&0));
    assert!(ids.contains(&1));

    let mut pos = 0usize;
    let mut saw_call_to_zero = false;
    while pos < linked.code.len() {
        let Ok((inst, next)) = Instruction::decode_at(&linked.code, pos) else {
            break;
        };
        if inst.opcode == Opcode::Call && inst.operands.first() == Some(&0) {
            saw_call_to_zero = true;
        }
        pos = next;
    }
    assert!(
        saw_call_to_zero,
        "Call operand should still target global function id 0 after link"
    );

    verify(&linked).expect("linked module verifies");
}

fn struct_type_record() -> TypeRecord {
    TypeRecord {
        type_id: 0,
        kind: TypeKind::Struct,
        aux: vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    }
}

fn enum_type_record() -> TypeRecord {
    TypeRecord {
        type_id: 0,
        kind: TypeKind::Enum,
        aux: vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    }
}

fn inst_at(code: &[u8], offset: u32) -> Instruction {
    let start = usize::try_from(offset).unwrap_or(0);
    let (inst, _) = Instruction::decode_at(code, start).expect("decode instruction");
    inst
}

#[test]
#[allow(clippy::too_many_lines)]
fn link_rebases_get_field_and_match_tag_type_operands() {
    let s32_kind = u32::from(PrimitiveKind::S32 as u8);
    let mut code_a = Instruction {
        opcode: Opcode::Const,
        operands: vec![0, s32_kind],
    }
    .encode();
    code_a.extend(
        Instruction {
            opcode: Opcode::MakeStruct,
            operands: vec![0, 1],
        }
        .encode(),
    );
    code_a.extend(
        Instruction {
            opcode: Opcode::GetField,
            operands: vec![0, 0],
        }
        .encode(),
    );
    code_a.extend(return_only());

    let mut code_b = Instruction {
        opcode: Opcode::Const,
        operands: vec![0, s32_kind],
    }
    .encode();
    code_b.extend(
        Instruction {
            opcode: Opcode::MakeEnum,
            operands: vec![0, 0, 1],
        }
        .encode(),
    );
    code_b.extend(
        Instruction {
            opcode: Opcode::MatchTag,
            operands: vec![0, 0],
        }
        .encode(),
    );
    code_b.extend(return_only());

    let pool = ConstPool {
        entries: vec![ConstEntry {
            tag: ConstTag::SignedInt,
            payload: 1i32.to_le_bytes().to_vec(),
        }],
    };

    let object_a = module_with_tables(
        0,
        code_a,
        2,
        pool.clone(),
        TypeTable {
            records: vec![struct_type_record()],
        },
    );
    let object_b = module_with_tables(
        1,
        code_b,
        2,
        pool,
        TypeTable {
            records: vec![enum_type_record()],
        },
    );

    let linked = link_modules(
        &[
            LinkInput {
                logical_path: "a::struct_mod".to_owned(),
                module: object_a,
            },
            LinkInput {
                logical_path: "b::enum_mod".to_owned(),
                module: object_b,
            },
        ],
        1,
    )
    .expect("link");

    let fn_a = linked
        .functions
        .functions
        .iter()
        .find(|f| f.function_id == 0)
        .expect("fn 0");
    let fn_b = linked
        .functions
        .functions
        .iter()
        .find(|f| f.function_id == 1)
        .expect("fn 1");

    let get_field_inst = inst_at(
        &linked.code,
        fn_a.code_offset.saturating_add(
            u32::try_from(
                Instruction {
                    opcode: Opcode::Const,
                    operands: vec![0, s32_kind],
                }
                .encode()
                .len()
                    + Instruction {
                        opcode: Opcode::MakeStruct,
                        operands: vec![0, 1],
                    }
                    .encode()
                    .len(),
            )
            .unwrap_or(0),
        ),
    );
    assert_eq!(get_field_inst.opcode, Opcode::GetField);
    assert_eq!(get_field_inst.operands.first(), Some(&0));

    let match_tag_inst = inst_at(
        &linked.code,
        fn_b.code_offset.saturating_add(
            u32::try_from(
                Instruction {
                    opcode: Opcode::Const,
                    operands: vec![0, s32_kind],
                }
                .encode()
                .len()
                    + Instruction {
                        opcode: Opcode::MakeEnum,
                        operands: vec![0, 0, 1],
                    }
                    .encode()
                    .len(),
            )
            .unwrap_or(0),
        ),
    );
    assert_eq!(match_tag_inst.opcode, Opcode::MatchTag);
    assert_eq!(
        match_tag_inst.operands.first(),
        Some(&1),
        "second module's local type id 0 should rebase to 1"
    );

    verify(&linked).expect("linked module verifies");
}

#[test]
fn link_rebases_make_str_const_operand() {
    let make_str = Instruction {
        opcode: Opcode::MakeStr,
        operands: vec![0],
    }
    .encode();
    let mut code_b = make_str;
    code_b.extend(return_only());

    let pool_a = ConstPool {
        entries: vec![ConstEntry {
            tag: ConstTag::Bytes,
            payload: b"module_a".to_vec(),
        }],
    };
    let pool_b = ConstPool {
        entries: vec![ConstEntry {
            tag: ConstTag::Bytes,
            payload: b"module_b".to_vec(),
        }],
    };

    let object_a = module_with_fn(0, return_only(), 0);
    let mut object_a = object_a;
    object_a.constants = pool_a;

    let object_b = module_with_tables(1, code_b, 1, pool_b, TypeTable::default());

    let linked = link_modules(
        &[
            LinkInput {
                logical_path: "a::strings".to_owned(),
                module: object_a,
            },
            LinkInput {
                logical_path: "b::strings".to_owned(),
                module: object_b,
            },
        ],
        1,
    )
    .expect("link");

    let fn_b = linked
        .functions
        .functions
        .iter()
        .find(|f| f.function_id == 1)
        .expect("fn 1");
    let make_str_inst = inst_at(&linked.code, fn_b.code_offset);
    assert_eq!(make_str_inst.opcode, Opcode::MakeStr);
    assert_eq!(
        make_str_inst.operands.first(),
        Some(&1),
        "second module's local const index 0 should rebase to 1"
    );

    verify(&linked).expect("linked module verifies");
}

#[test]
fn link_preserves_return_type_id_sentinels() {
    let unit_type = TypeRecord {
        type_id: 0,
        kind: TypeKind::Unit,
        aux: vec![],
    };
    let object_a = module_with_tables(
        0,
        return_only(),
        0,
        ConstPool::default(),
        TypeTable {
            records: vec![unit_type],
        },
    );
    let mut object_b = module_with_fn(1, return_only(), 0);
    object_b.functions.functions[0].return_type_id = 1;

    let linked = link_modules(
        &[
            LinkInput {
                logical_path: "a::unit_type".to_owned(),
                module: object_a,
            },
            LinkInput {
                logical_path: "b::caller".to_owned(),
                module: object_b,
            },
        ],
        1,
    )
    .expect("link");

    let fn_b = linked
        .functions
        .functions
        .iter()
        .find(|f| f.function_id == 1)
        .expect("fn 1");
    assert_eq!(
        fn_b.return_type_id, 1,
        "sentinel return_type_id 1 must not pick up prior module type_base"
    );
    verify(&linked).expect("linked module verifies");
}

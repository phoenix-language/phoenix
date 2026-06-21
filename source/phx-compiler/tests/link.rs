//! PHX0 linker: global `function_id` stability and `Call` operands after merge.

mod support;

use std::collections::{HashMap, HashSet};

use phx_bytecode::ENTRY_NONE;
use phx_bytecode::{
    BytecodeModule, ConstEntry, ConstPool, ConstTag, FileHeader, FunctionRecord, FunctionTable,
    Instruction, LocalLayoutTable, Opcode, PHX0_HAS_DEBUG, PcSpanTable, PrimitiveKind, TypeKind,
    TypeRecord, TypeTable, verify,
};
use phx_compiler::{
    BuildProfile, LinkInput, compile_source_with_module_root, link_modules,
    unstable::{self, IrFunction, IrInst, IrModule},
};
use support::{
    module_entry_source, modules_fixture_root_and_entry, test_encode, test_ok, test_some,
};

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
        pc_spans: PcSpanTable::default(),
    }
}

fn return_only() -> Vec<u8> {
    test_encode(&Instruction {
        opcode: Opcode::Return,
        operands: vec![],
    })
}

fn pop_then_return() -> Vec<u8> {
    let mut code = test_encode(&Instruction {
        opcode: Opcode::Pop,
        operands: vec![],
    });
    code.extend(return_only());
    code
}

fn call_then_return(callee: u32) -> Vec<u8> {
    let mut code = test_encode(&Instruction {
        opcode: Opcode::Call,
        operands: vec![callee],
    });
    code.extend(pop_then_return());
    code
}

#[test]
fn link_preserves_call_function_id_operands() {
    let object_a = module_with_fn(0, return_only(), 0);
    let object_b = module_with_fn(1, call_then_return(0), 1);

    let linked = test_ok(
        link_modules(
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
        ),
        "link",
    );

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

    test_ok(verify(&linked), "linked module verifies");
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
    let (inst, _) = test_ok(Instruction::decode_at(code, start), "decode instruction");
    inst
}

fn link_two(inputs: &[LinkInput], entry: u32) -> BytecodeModule {
    test_ok(link_modules(inputs, entry), "link")
}

fn function_by_id(linked: &BytecodeModule, id: u32) -> &FunctionRecord {
    test_some(
        linked
            .functions
            .functions
            .iter()
            .find(|f| f.function_id == id),
        &format!("fn {id}"),
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn link_rebases_get_field_and_match_tag_type_operands() {
    let s32_kind = u32::from(PrimitiveKind::S32 as u8);
    let mut code_a = test_encode(&Instruction {
        opcode: Opcode::Const,
        operands: vec![0, s32_kind],
    });
    code_a.extend(test_encode(&Instruction {
        opcode: Opcode::MakeStruct,
        operands: vec![0, 1],
    }));
    code_a.extend(test_encode(&Instruction {
        opcode: Opcode::GetField,
        operands: vec![0, 0],
    }));
    code_a.extend(pop_then_return());

    let mut code_b = test_encode(&Instruction {
        opcode: Opcode::Const,
        operands: vec![0, s32_kind],
    });
    code_b.extend(test_encode(&Instruction {
        opcode: Opcode::MakeEnum,
        operands: vec![0, 0, 1],
    }));
    code_b.extend(test_encode(&Instruction {
        opcode: Opcode::MatchTag,
        operands: vec![0, 0],
    }));
    code_b.extend(pop_then_return());

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

    let linked = link_two(
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
    );

    let fn_a = function_by_id(&linked, 0);
    let fn_b = function_by_id(&linked, 1);

    let const_len = test_encode(&Instruction {
        opcode: Opcode::Const,
        operands: vec![0, s32_kind],
    })
    .len();
    let struct_len = test_encode(&Instruction {
        opcode: Opcode::MakeStruct,
        operands: vec![0, 1],
    })
    .len();
    let get_field_inst = inst_at(
        &linked.code,
        fn_a.code_offset
            .saturating_add(u32::try_from(const_len + struct_len).unwrap_or(0)),
    );
    assert_eq!(get_field_inst.opcode, Opcode::GetField);
    assert_eq!(get_field_inst.operands.first(), Some(&0));

    let enum_len = test_encode(&Instruction {
        opcode: Opcode::MakeEnum,
        operands: vec![0, 0, 1],
    })
    .len();
    let match_tag_inst = inst_at(
        &linked.code,
        fn_b.code_offset
            .saturating_add(u32::try_from(const_len + enum_len).unwrap_or(0)),
    );
    assert_eq!(match_tag_inst.opcode, Opcode::MatchTag);
    assert_eq!(
        match_tag_inst.operands.first(),
        Some(&1),
        "second module's local type id 0 should rebase to 1"
    );

    test_ok(verify(&linked), "linked module verifies");
}

#[test]
fn link_rebases_make_str_const_operand() {
    let make_str = test_encode(&Instruction {
        opcode: Opcode::MakeStr,
        operands: vec![0],
    });
    let mut code_b = make_str;
    code_b.extend(pop_then_return());

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

    let linked = link_two(
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
    );

    let fn_b = function_by_id(&linked, 1);
    let make_str_inst = inst_at(&linked.code, fn_b.code_offset);
    assert_eq!(make_str_inst.opcode, Opcode::MakeStr);
    assert_eq!(
        make_str_inst.operands.first(),
        Some(&1),
        "second module's local const index 0 should rebase to 1"
    );

    test_ok(verify(&linked), "linked module verifies");
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
    let mut object_b = module_with_tables(
        1,
        {
            let s32_kind = u32::from(PrimitiveKind::S32 as u8);
            let mut code = test_encode(&Instruction {
                opcode: Opcode::Const,
                operands: vec![0, s32_kind],
            });
            code.extend(return_only());
            code
        },
        1,
        ConstPool {
            entries: vec![ConstEntry {
                tag: ConstTag::SignedInt,
                payload: 0i32.to_le_bytes().to_vec(),
            }],
        },
        TypeTable::default(),
    );
    object_b.functions.functions[0].return_type_id = 1;

    let linked = link_two(
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
    );

    let fn_b = function_by_id(&linked, 1);
    assert_eq!(
        fn_b.return_type_id, 1,
        "sentinel return_type_id 1 must not pick up prior module type_base"
    );
    test_ok(verify(&linked), "linked module verifies");
}

fn compile_module_tree(tree_name: &str) -> unstable::TypedProgram {
    let (root, entry) = modules_fixture_root_and_entry(tree_name);
    let (_, source) = module_entry_source(tree_name);
    test_ok(
        compile_source_with_module_root(source, &entry, &root),
        &format!("compile {tree_name}"),
    )
    .typed
}

fn lower_module_ir(
    typed: &unstable::TypedProgram,
    full: &IrModule,
    module_id: u32,
) -> Option<IrModule> {
    let mut functions: Vec<IrFunction> = full
        .functions
        .iter()
        .filter(|f| {
            typed
                .resolved
                .defs
                .get(f.def.index() as usize)
                .is_some_and(|d| d.module == module_id)
        })
        .cloned()
        .collect();
    if functions.is_empty() {
        return None;
    }
    let entry = typed.entry.filter(|&main| {
        typed
            .resolved
            .defs
            .get(main.index() as usize)
            .is_some_and(|d| d.module == module_id)
    });

    let all_constants = &full.constants;
    let mut used = HashSet::new();
    for func in &functions {
        for block in &func.blocks {
            for spanned in &block.insts {
                match &spanned.inst {
                    IrInst::Const { index, .. } | IrInst::MakeStr { pool_index: index } => {
                        used.insert(*index);
                    }
                    #[allow(clippy::wildcard_enum_match_arm)]
                    _ => {}
                }
            }
        }
    }
    let mut ordered: Vec<u32> = used.into_iter().collect();
    ordered.sort_unstable();
    let mut constants = Vec::with_capacity(ordered.len());
    let mut remap = HashMap::new();
    for old in ordered {
        let new = u32::try_from(constants.len()).unwrap_or(u32::MAX);
        remap.insert(old, new);
        if let Some(entry) = all_constants.get(old as usize) {
            constants.push(entry.clone());
        }
    }
    for func in &mut functions {
        for block in &mut func.blocks {
            for spanned in &mut block.insts {
                match &mut spanned.inst {
                    IrInst::Const { index, .. } | IrInst::MakeStr { pool_index: index } => {
                        if let Some(&new) = remap.get(index) {
                            *index = new;
                        }
                    }
                    #[allow(clippy::wildcard_enum_match_arm)]
                    _ => {}
                }
            }
        }
    }

    Some(IrModule {
        functions,
        entry,
        constants,
    })
}

fn codegen_link_inputs_for_profile(
    typed: &unstable::TypedProgram,
    profile: BuildProfile,
) -> Vec<LinkInput> {
    let full_ir = test_ok(unstable::lower(typed), "lower");
    let global_fn: HashMap<_, u32> = full_ir
        .functions
        .iter()
        .map(|f| (f.def, f.id.index()))
        .collect();

    typed
        .resolved
        .modules
        .iter()
        .filter_map(|src| {
            let module_ir = lower_module_ir(typed, &full_ir, src.id)?;
            let rel_source = src
                .filesystem
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(src.logical_path.as_str());
            let module = test_ok(
                unstable::codegen_module(
                    &module_ir,
                    typed,
                    &global_fn,
                    src.id == typed.resolved.root,
                    Some(rel_source),
                    profile,
                ),
                "codegen_module",
            );
            Some(LinkInput {
                logical_path: src.logical_path.clone(),
                module,
            })
        })
        .collect()
}

fn link_codegen_modules(profile: BuildProfile, tree_name: &str) -> BytecodeModule {
    let typed = compile_module_tree(tree_name);
    let inputs = codegen_link_inputs_for_profile(&typed, profile);
    assert!(
        inputs.len() >= 2,
        "expected at least two codegen objects for multi-module link test, got {}",
        inputs.len()
    );
    let entry_fn = typed
        .entry
        .and_then(|main| {
            let full_ir = test_ok(unstable::lower(&typed), "lower");
            full_ir
                .functions
                .iter()
                .find(|f| f.def == main)
                .map(|f| f.id.index())
        })
        .unwrap_or(ENTRY_NONE);
    test_ok(link_modules(&inputs, entry_fn), "link")
}

#[test]
fn link_release_profile_strips_merged_debug_sections() {
    let linked = link_codegen_modules(BuildProfile::Release, "main_list");

    assert!(
        linked.pc_spans.entries.is_empty(),
        "release link should omit merged PC span rows"
    );
    assert_eq!(
        linked.header.flags & PHX0_HAS_DEBUG,
        0,
        "release link should clear PHX0_HAS_DEBUG"
    );
    assert_eq!(
        linked.header.section_count, 5,
        "release link should omit section 5"
    );
    test_ok(verify(&linked), "linked release module verifies");

    let bytes = test_ok(linked.encode(), "encode");
    let decoded = test_ok(BytecodeModule::decode(&bytes), "decode");
    test_ok(verify(&decoded), "decoded release link verifies");
    assert!(decoded.pc_spans.entries.is_empty());
    assert_eq!(decoded.header.flags & PHX0_HAS_DEBUG, 0);
    assert_eq!(decoded.header.section_count, 5);
}

#[test]
fn link_dev_profile_keeps_merged_debug_sections() {
    let linked = link_codegen_modules(BuildProfile::Dev, "main_list");

    assert!(
        !linked.pc_spans.entries.is_empty(),
        "dev link should keep merged PC span rows"
    );
    assert!(
        linked.pc_spans.lookup_function_name(0).is_some(),
        "dev link should keep merged function debug names"
    );
    assert_ne!(
        linked.header.flags & PHX0_HAS_DEBUG,
        0,
        "dev link should set PHX0_HAS_DEBUG"
    );
    assert_eq!(linked.header.section_count, 6);
    test_ok(verify(&linked), "linked dev module verifies");
}

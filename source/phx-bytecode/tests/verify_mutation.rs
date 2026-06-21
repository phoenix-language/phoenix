//! Byte-mutation tests: corrupted modules must fail verification and must not panic the VM.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use phx_bytecode::{
    BytecodeModule, ConstEntry, ConstPool, ConstTag, FileHeader, FunctionRecord, FunctionTable,
    Instruction, LocalLayoutTable, ModuleError, Opcode, PC_SPAN_SUB_VERSION, PHX0_HAS_DEBUG,
    PcSpanEntry, PcSpanError, PcSpanTable, PrimitiveKind, SectionEntry, SectionError, SectionKind,
    TypeTable, VerifyError, verify,
};
use phx_vm::run_unverified;
use support::{
    const_return_code, join_depth_mismatch_code, minimal_module, valid_const_return_module,
};

fn assert_verify_rejects(module: &BytecodeModule) {
    assert!(
        verify(module).is_err(),
        "expected verify to reject mutated module"
    );
}

/// Defense-in-depth: malformed bytecode must return [`Err`], never panic.
fn assert_run_returns_err(module: &BytecodeModule) {
    assert!(
        run_unverified(module).is_err(),
        "expected VM run to return Err on malformed bytecode"
    );
}

fn assert_run_does_not_panic(module: &BytecodeModule) {
    let _ = run_unverified(module);
}

const HEADER_SIZE: usize = 24;
const SECTION_ENTRY_SIZE: usize = 12;

fn section_payload_range(bytes: &[u8], kind: SectionKind) -> std::ops::Range<usize> {
    let section_count = u32::from_le_bytes(
        bytes[12..16]
            .try_into()
            .expect("header section_count bytes"),
    );
    let section_count = usize::try_from(section_count).expect("section_count usize");
    for index in 0..section_count {
        let start = HEADER_SIZE + index * SECTION_ENTRY_SIZE;
        let entry = SectionEntry::decode(
            bytes[start..start + SECTION_ENTRY_SIZE]
                .try_into()
                .expect("section entry bytes"),
        )
        .expect("decode section entry");
        if entry.kind == kind {
            let offset = usize::try_from(entry.offset).expect("section offset usize");
            let length = usize::try_from(entry.length).expect("section length usize");
            return offset..offset + length;
        }
    }
    panic!("missing {kind:?} section");
}

fn section_length_slot(kind: SectionKind, bytes: &[u8]) -> std::ops::Range<usize> {
    let section_count = u32::from_le_bytes(
        bytes[12..16]
            .try_into()
            .expect("header section_count bytes"),
    );
    let section_count = usize::try_from(section_count).expect("section_count usize");
    for index in 0..section_count {
        let start = HEADER_SIZE + index * SECTION_ENTRY_SIZE;
        let entry = SectionEntry::decode(
            bytes[start..start + SECTION_ENTRY_SIZE]
                .try_into()
                .expect("section entry bytes"),
        )
        .expect("decode section entry");
        if entry.kind == kind {
            return start + 8..start + 12;
        }
    }
    panic!("missing {kind:?} section");
}

fn debug_module_with_pc_spans() -> BytecodeModule {
    let mut module = valid_const_return_module();
    module.pc_spans = PcSpanTable {
        files: vec!["main.phx".to_owned()],
        entries: vec![PcSpanEntry::new(0, 0, 0, 4, 9)],
        function_names: Vec::new(),
    };
    module
}

#[test]
fn mutate_code_unknown_opcode_rejected() {
    let mut module = valid_const_return_module();
    assert!(!module.code.is_empty());
    module.code[0] = 0xFF;
    assert_verify_rejects(&module);
    assert_run_returns_err(&module);
}

#[test]
fn mutate_jump_target_out_of_range_rejected_by_verifier() {
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
            opcode: Opcode::JumpIfTrue,
            operands: vec![99],
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
    let module = minimal_module(code, 4, 0, 0);
    assert_verify_rejects(&module);
    // Invalid jumps are verify-only; unverified execution must not panic.
    assert_run_does_not_panic(&module);
}

#[test]
fn mutate_jump_zero_operands_rejected() {
    let mut code = Vec::new();
    code.extend(
        Instruction {
            opcode: Opcode::Jump,
            operands: vec![],
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
    let module = minimal_module(code, 4, 0, 0);
    assert_verify_rejects(&module);
    // Do not run the VM: zero-operand `Jump` defaults target to 0 and loops forever.
}

#[test]
fn mutate_invalid_call_target_rejected() {
    let mut code = Vec::new();
    code.extend(
        Instruction {
            opcode: Opcode::Call,
            operands: vec![99],
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
    let module = minimal_module(code, 8, 0, 0);
    assert_verify_rejects(&module);
    assert_run_returns_err(&module);
}

#[test]
fn mutate_local_count_zero_with_load_rejected() {
    let mut code = Vec::new();
    code.extend(
        Instruction {
            opcode: Opcode::LoadLocal,
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
    let module = minimal_module(code, 4, 0, 0);
    assert_verify_rejects(&module);
    assert_run_returns_err(&module);
}

#[test]
fn mutate_stack_max_too_low_rejected_by_verifier() {
    let module = minimal_module(const_return_code(), 0, 0, 1);
    assert_verify_rejects(&module);
    // Verifier-only invariant: MVP VM does not re-check `stack_max` at run time.
    assert_run_does_not_panic(&module);
}

#[test]
fn mutate_truncated_code_length_rejected_by_verifier() {
    let mut module = valid_const_return_module();
    module.functions.functions[0].code_len =
        module.functions.functions[0].code_len.saturating_add(4);
    assert_verify_rejects(&module);
    // Metadata/code length mismatch is caught at verify; VM executes the bytes present.
    assert_run_does_not_panic(&module);
}

#[test]
fn mutate_jump_if_false_branch_underflow_rejected() {
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
            opcode: Opcode::JumpIfFalse,
            operands: vec![0],
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
    let branch_off = u32::try_from(code.len()).expect("offset");
    code.extend(
        Instruction {
            opcode: Opcode::Add,
            operands: vec![u32::from(PrimitiveKind::S32.as_u8())],
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

    let mut instructions = Vec::new();
    let mut off = 0usize;
    while off < code.len() {
        let (inst, next) = Instruction::decode_at(&code, off).expect("decode");
        instructions.push((u32::try_from(off).expect("offset"), inst));
        off = next;
    }
    instructions[1].1.operands[0] = branch_off;
    let mut patched = Vec::new();
    for (_, inst) in &instructions {
        patched.extend(inst.encode().expect("encode"));
    }

    let module = jump_if_false_underflow_module(patched);
    assert_verify_rejects(&module);
    assert_run_returns_err(&module);
}

fn jump_if_false_underflow_module(code: Vec<u8>) -> BytecodeModule {
    BytecodeModule {
        header: FileHeader::new(5, 0),
        constants: ConstPool {
            entries: vec![ConstEntry {
                tag: ConstTag::Bool,
                payload: vec![0],
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
        local_layouts: LocalLayoutTable::default(),
        pc_spans: PcSpanTable::default(),
    }
}

#[test]
fn mutate_stack_underflow_rejected() {
    let mut code = Vec::new();
    code.extend(
        Instruction {
            opcode: Opcode::Add,
            operands: vec![u32::from(PrimitiveKind::S32.as_u8())],
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
    let module = minimal_module(code, 4, 0, 0);
    assert_verify_rejects(&module);
    assert_run_returns_err(&module);
}

#[test]
fn heap_s32_slice_index_store_load_roundtrip() {
    use phx_bytecode::ScalarValue;
    use phx_vm::{Value, run_captured};

    let module = support::heap_s32_slice_index_roundtrip_module();
    let verified = verify(&module).expect("verify s32 slice roundtrip");
    let capture = run_captured(verified).expect("run s32 slice roundtrip");
    let loaded = capture
        .main_local(1)
        .and_then(|v| match v {
            Value::Scalar(ScalarValue::I32(n)) => Some(n),
            _ => None,
        })
        .expect("slot 1 should hold loaded s32");
    assert_eq!(loaded, 1);
}

#[test]
fn heap_alloc_ptr_store_load_roundtrip() {
    use phx_bytecode::ScalarValue;
    use phx_vm::{Value, run_captured};

    let module = support::heap_alloc_roundtrip_module();
    let verified = verify(&module).expect("verify heap alloc roundtrip");
    let capture = run_captured(verified).expect("run heap alloc roundtrip");
    let loaded = capture
        .main_local(1)
        .and_then(|v| match v {
            Value::Scalar(ScalarValue::U8(n)) => Some(n),
            _ => None,
        })
        .expect("slot 1 should hold loaded u8");
    assert_eq!(loaded, 77);
}

#[test]
fn mutate_truncated_file_bytes_rejected_at_decode() {
    let module = valid_const_return_module();
    let bytes = module.encode().expect("encode");
    let truncated = &bytes[..bytes.len().saturating_sub(4)];
    let err = BytecodeModule::decode(truncated).unwrap_err();
    assert!(matches!(
        err,
        ModuleError::Truncated | ModuleError::SectionOutOfBounds
    ));
}

#[test]
fn round_trip_valid_module_passes_verify() {
    let module = valid_const_return_module();
    let bytes = module.encode().expect("encode");
    let decoded = BytecodeModule::decode(&bytes).expect("decode");
    verify(&decoded).expect("verify round-trip");
}

#[test]
fn stripped_module_without_debug_flag_verifies() {
    let module = valid_const_return_module();
    assert_eq!(module.header.flags & PHX0_HAS_DEBUG, 0);
    assert!(module.pc_spans.is_empty());

    let bytes = module.encode().expect("encode stripped module");
    let decoded = BytecodeModule::decode(&bytes).expect("decode stripped module");

    assert_eq!(decoded.header.flags & PHX0_HAS_DEBUG, 0);
    assert!(decoded.pc_spans.is_empty());
    verify(&decoded).expect("verify stripped module");
}

#[test]
fn mutate_section5_truncated_payload_rejected_at_decode() {
    let module = debug_module_with_pc_spans();
    let mut bytes = module.encode().expect("encode");
    let symbols_range = section_payload_range(&bytes, SectionKind::Symbols);
    let symbols_len = symbols_range.end - symbols_range.start;
    let new_len = u32::try_from(symbols_len.saturating_sub(4)).expect("shortened symbols length");
    let length_slot = section_length_slot(SectionKind::Symbols, &bytes);
    bytes[length_slot].copy_from_slice(&new_len.to_le_bytes());

    let err = BytecodeModule::decode(&bytes).expect_err("truncated section 5");
    assert!(matches!(err, ModuleError::PcSpans(PcSpanError::Truncated)));
}

#[test]
fn mutate_section5_bad_sub_version_rejected_at_decode() {
    let module = debug_module_with_pc_spans();
    let mut bytes = module.encode().expect("encode");
    let symbols_range = section_payload_range(&bytes, SectionKind::Symbols);
    bytes[symbols_range.start..symbols_range.start + 4]
        .copy_from_slice(&(PC_SPAN_SUB_VERSION + 1).to_le_bytes());

    let err = BytecodeModule::decode(&bytes).expect_err("bad section 5 sub-version");
    assert!(matches!(
        err,
        ModuleError::PcSpans(PcSpanError::UnsupportedSubVersion { found })
            if found == PC_SPAN_SUB_VERSION + 1
    ));
}

#[test]
fn mutate_section5_overlapping_entries_rejected_at_decode() {
    let module = debug_module_with_pc_spans();
    let mut bytes = module.encode().expect("encode");
    let symbols_range = section_payload_range(&bytes, SectionKind::Symbols);
    let original = module.pc_spans.encode();

    let duplicate_entry = PcSpanEntry::new(0, 0, 0, 12, 15);
    let mut payload = Vec::new();
    payload.extend_from_slice(&PC_SPAN_SUB_VERSION.to_le_bytes());
    payload.extend_from_slice(&1u32.to_le_bytes());
    payload.extend_from_slice(&8u32.to_le_bytes());
    payload.extend_from_slice(b"main.phx");
    payload.extend_from_slice(&2u32.to_le_bytes());
    payload.extend_from_slice(&original[24..44]);
    payload.extend_from_slice(&duplicate_entry.function_id.to_le_bytes());
    payload.extend_from_slice(&duplicate_entry.pc.to_le_bytes());
    payload.extend_from_slice(&duplicate_entry.file_id.to_le_bytes());
    payload.extend_from_slice(&duplicate_entry.span_start.to_le_bytes());
    payload.extend_from_slice(&duplicate_entry.span_end.to_le_bytes());
    payload.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(payload.len(), original.len() + 20);

    bytes.splice(symbols_range, payload);
    let length_slot = section_length_slot(SectionKind::Symbols, &bytes);
    let new_len = u32::try_from(original.len() + 20).expect("expanded symbols length");
    bytes[length_slot].copy_from_slice(&new_len.to_le_bytes());

    let err = BytecodeModule::decode(&bytes).expect_err("overlapping section 5 rows");
    assert!(matches!(
        err,
        ModuleError::PcSpans(PcSpanError::OverlappingEntries {
            function_id: 0,
            pc: 0
        })
    ));
}

#[test]
fn mutate_unknown_function_debug_name_rejected() {
    let mut module = debug_module_with_pc_spans();
    module.pc_spans.push_function_name(99, "missing".to_owned());
    assert_verify_rejects(&module);
}

#[test]
fn mutate_unsupported_version_rejected() {
    let mut module = valid_const_return_module();
    module.header.version_major = 99;
    assert_verify_rejects(&module);
}

#[test]
fn mutate_duplicate_section_kind_rejected_at_decode() {
    let module = valid_const_return_module();
    let mut bytes = module.encode().expect("encode");
    // Second section table row (Types) — patch kind tag to Constants (1).
    bytes[36] = 1;
    bytes[37] = 0;
    let err = BytecodeModule::decode(&bytes).unwrap_err();
    assert!(matches!(
        err,
        ModuleError::Section(SectionError::DuplicateKind(SectionKind::Constants))
    ));
}

#[test]
fn mutate_overlapping_sections_rejected_at_decode() {
    let module = valid_const_return_module();
    let mut bytes = module.encode().expect("encode");
    // First section row (Constants): read offset at bytes[28..32].
    let constants_offset = u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]);
    // Second section row (Types): set same offset so payloads overlap.
    bytes[40..44].copy_from_slice(&constants_offset.to_le_bytes());
    let err = BytecodeModule::decode(&bytes).unwrap_err();
    assert!(matches!(
        err,
        ModuleError::Section(SectionError::OverlappingSections {
            first: SectionKind::Constants,
            second: SectionKind::Types,
        })
    ));
}

#[test]
fn mutate_join_depth_mismatch_rejected() {
    let (code, merge_off) = join_depth_mismatch_code();
    let mut module = minimal_module(code, 4, 0, 0);
    module.constants.entries = vec![
        ConstEntry {
            tag: ConstTag::Bool,
            payload: vec![1],
        },
        ConstEntry {
            tag: ConstTag::SignedInt,
            payload: 1i32.to_le_bytes().to_vec(),
        },
    ];
    let err = verify(&module).expect_err("join depth mismatch");
    assert!(
        matches!(
            err,
            VerifyError::JoinDepthMismatch {
                function_id: 0,
                offset,
                expected: 0,
                found: 1,
            } if offset == merge_off
        ),
        "unexpected error: {err:?}"
    );
    assert_run_does_not_panic(&module);
}

#[test]
fn mutate_jump_target_mid_instruction_rejected() {
    let mut code = Vec::new();
    code.extend(
        Instruction {
            opcode: Opcode::Const,
            operands: vec![0, u32::from(PrimitiveKind::S32.as_u8())],
        }
        .encode()
        .expect("encode"),
    );
    // Target 3 lands inside the first `Const` operand, not on an instruction boundary.
    code.extend(
        Instruction {
            opcode: Opcode::JumpIfTrue,
            operands: vec![3],
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
    let module = minimal_module(code, 4, 0, 0);
    let err = verify(&module).expect_err("mid-instruction jump");
    assert!(matches!(
        err,
        VerifyError::InvalidJumpTarget {
            function_id: 0,
            target: 3,
            ..
        }
    ));
    assert_run_does_not_panic(&module);
}

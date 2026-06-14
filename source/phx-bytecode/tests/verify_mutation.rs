//! Byte-mutation tests: corrupted modules must fail verification and must not panic the VM.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use phx_bytecode::{
    BytecodeModule, ConstEntry, ConstPool, ConstTag, FileHeader, FunctionRecord, FunctionTable,
    Instruction, LocalLayoutTable, ModuleError, Opcode, PrimitiveKind, TypeTable, verify,
};
use phx_vm::run;
use support::{const_return_code, minimal_module, valid_const_return_module};

fn assert_verify_rejects(module: &BytecodeModule) {
    assert!(
        verify(module).is_err(),
        "expected verify to reject mutated module"
    );
}

/// Defense-in-depth: malformed bytecode must return [`Err`], never panic.
fn assert_run_returns_err(module: &BytecodeModule) {
    assert!(
        run(module).is_err(),
        "expected VM run to return Err on malformed bytecode"
    );
}

fn assert_run_does_not_panic(module: &BytecodeModule) {
    let _ = run(module);
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
        .encode(),
    );
    code.extend(
        Instruction {
            opcode: Opcode::JumpIfTrue,
            operands: vec![99],
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
    let module = minimal_module(code, 4, 0, 0);
    assert_verify_rejects(&module);
    // MVP VM treats falling off the end of `main` as normal return; invalid jumps are verify-only.
    assert_run_does_not_panic(&module);
}

#[test]
fn mutate_invalid_call_target_rejected() {
    let mut code = Vec::new();
    code.extend(
        Instruction {
            opcode: Opcode::Call,
            operands: vec![99],
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
        .encode(),
    );
    code.extend(
        Instruction {
            opcode: Opcode::Return,
            operands: vec![],
        }
        .encode(),
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
        .encode(),
    );
    code.extend(
        Instruction {
            opcode: Opcode::JumpIfFalse,
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
    let branch_off = u32::try_from(code.len()).expect("offset");
    code.extend(
        Instruction {
            opcode: Opcode::Add,
            operands: vec![u32::from(PrimitiveKind::S32.as_u8())],
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
        patched.extend(inst.encode());
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
        .encode(),
    );
    code.extend(
        Instruction {
            opcode: Opcode::Return,
            operands: vec![],
        }
        .encode(),
    );
    let module = minimal_module(code, 4, 0, 0);
    assert_verify_rejects(&module);
    assert_run_returns_err(&module);
}

#[test]
fn heap_alloc_ptr_store_load_roundtrip() {
    use phx_bytecode::ScalarValue;
    use phx_vm::{Value, run_captured};

    let module = support::heap_alloc_roundtrip_module();
    verify(&module).expect("verify heap alloc roundtrip");
    let capture = run_captured(&module).expect("run heap alloc roundtrip");
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

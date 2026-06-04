//! PHX0 linker: global `function_id` stability and `Call` operands after merge.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_bytecode::{
    BytecodeModule, FileHeader, FunctionRecord, FunctionTable, Instruction, LocalLayoutTable,
    Opcode, TypeTable, verify,
};
use phx_bytecode::{ConstPool, ENTRY_NONE};
use phx_compiler::{LinkInput, link_modules};

fn module_with_fn(fn_id: u32, code: Vec<u8>, stack_max: u16) -> BytecodeModule {
    let code_len = u32::try_from(code.len()).unwrap_or(0);
    BytecodeModule {
        header: FileHeader::new(5, ENTRY_NONE),
        constants: ConstPool::default(),
        types: TypeTable::default(),
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

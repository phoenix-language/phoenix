//! Codegen integration tests.

use std::path::Path;

use phx_bytecode::verify;
use phx_bytecode::{BytecodeModule, ConstTag, Opcode};
use phx_compiler::{codegen, compile_source, lower};

#[test]
fn codegen_sample_round_trip_and_verify() {
    let source = include_str!("../../../tests/cli/fixtures/sample.phx");
    let unit = compile_source(source, Some(Path::new("sample.phx")))
        .unwrap_or_else(|e| panic!("compile: {e}"));
    let ir = lower(&unit.typed);
    let module = codegen(&ir);

    assert_eq!(module.functions.functions.len(), 2);
    assert!(!module.code.is_empty());
    assert!(module.constants.entries.len() >= 2);

    let main_id = module.header.entry_function_id;
    let main_rec = module
        .functions
        .functions
        .iter()
        .find(|f| f.function_id == main_id)
        .expect("main function record");
    assert_eq!(main_rec.arity, 0);

    let bytes = module.encode();
    let decoded = BytecodeModule::decode(&bytes).expect("decode");
    verify(&decoded).expect("verify");

    let add_rec = module
        .functions
        .functions
        .iter()
        .find(|f| f.function_id != main_id)
        .expect("add");
    assert_eq!(add_rec.arity, 2);

    let add_code = &module.code
        [add_rec.code_offset as usize..add_rec.code_offset as usize + add_rec.code_len as usize];
    assert!(add_code.contains(&Opcode::Add.as_u8()));
}

#[test]
fn codegen_constants_include_sample_literals() {
    let source = include_str!("../../../tests/cli/fixtures/sample.phx");
    let unit = compile_source(source, None).unwrap();
    let module = codegen(&lower(&unit.typed));

    let mut has_ten = false;
    let mut has_two = false;
    for entry in &module.constants.entries {
        if entry.tag == ConstTag::SignedInt && entry.payload.len() >= 8 {
            let v = i64::from_le_bytes(entry.payload[0..8].try_into().unwrap());
            if v == 10 {
                has_ten = true;
            }
            if v == 2 {
                has_two = true;
            }
        }
    }
    assert!(has_ten && has_two);
}

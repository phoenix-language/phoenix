//! V0-042 `std_try` fixture build and VM run.

#![allow(clippy::expect_used)]

use phx_bytecode::{Instruction, Opcode, verify};
use phx_compiler::{BuildOptions, build_project, load_project_binary};
use phx_test::{discover_cli_project, require_cli_project};
use phx_vm::run;

#[test]
fn std_try_fixture_runs() {
    let root = require_cli_project("std_try");
    let config = discover_cli_project(&root);
    build_project(&config, None, BuildOptions::force(true)).expect("build std_try");
    let module = load_project_binary(&config).expect("load bytecode");
    let verified = verify(&module).expect("verify");

    let entry_id = module.header.entry_function_id;
    let read_config = module
        .functions
        .functions
        .iter()
        .find(|f| f.function_id != entry_id)
        .expect("read_config function record");
    let start = read_config.code_offset as usize;
    let end = start + read_config.code_len as usize;
    let code = &module.code[start..end];
    let mut off = 0usize;
    while off < code.len() {
        let (inst, next) = Instruction::decode_at(code, off).expect("decode");
        if inst.opcode == Opcode::Jump {
            let target = inst.operands[0] as usize;
            assert!(
                target < code.len(),
                "jump target {target} out of range (code len {})",
                code.len()
            );
        }
        off = next;
    }

    run(verified).expect("run std_try");
}

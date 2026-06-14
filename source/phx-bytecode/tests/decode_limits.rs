//! Decode-time rejection of hostile untrusted entry counts (PHX-048).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use phx_bytecode::{
    BytecodeModule, ConstPool, FunctionTable, InstrError, Instruction, LocalLayoutTable, Opcode,
    TypeTable,
};
use support::valid_const_return_module;

/// Constants payload offset: 24-byte header + 5×12 section table.
const CONSTANTS_PAYLOAD_OFFSET: usize = 84;

fn huge_count_prefix() -> [u8; 4] {
    0xFFFF_FFFF_u32.to_le_bytes()
}

#[test]
fn const_pool_huge_count_rejected() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&huge_count_prefix());
    assert!(ConstPool::decode(&payload).is_err());
}

#[test]
fn types_huge_count_rejected() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&huge_count_prefix());
    assert!(TypeTable::decode(&payload).is_err());
}

#[test]
fn local_layout_huge_count_rejected() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&huge_count_prefix());
    assert!(LocalLayoutTable::decode(&payload).is_err());
}

#[test]
fn function_table_huge_count_rejected() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&huge_count_prefix());
    assert!(FunctionTable::decode(&payload).is_err());
}

#[test]
fn module_huge_constants_count_rejected() {
    let module = valid_const_return_module();
    let mut bytes = module.encode().expect("encode");
    bytes[CONSTANTS_PAYLOAD_OFFSET..CONSTANTS_PAYLOAD_OFFSET + 4]
        .copy_from_slice(&huge_count_prefix());
    assert!(BytecodeModule::decode(&bytes).is_err());
}

#[test]
fn instruction_operand_count_exceeds_remaining() {
    // Valid opcode, operand count 1, but no operand word follows.
    let bytes = [Opcode::Const.as_u8(), 1];
    let err = Instruction::decode_at(&bytes, 0).unwrap_err();
    assert_eq!(err, InstrError::Truncated);
}

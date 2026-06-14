//! Phoenix bytecode — `PHX0` encode/decode, opcode definitions, and verification.
//!
//! Portable output of the compiler; consumed by the `phx_vm` crate after the verifier pass.
//! Format contract: `docs/design/features/vm-linear.md`.
//!
//! Call [`verify`] on every image before execution; the MVP VM assumes invariants checked there.

mod cast;
mod const_pool;
mod decode;
mod encode;
mod function;
mod header;
mod instr;
mod local_layout;
mod module;
mod opcode;
mod scalar;
mod section;
mod stack_effect;
mod stack_flow;
mod types;
mod verify;

pub use cast::{PrimitiveKind, SLOT_KIND_AGG, SLOT_KIND_FN_PTR};
pub use const_pool::{ConstEntry, ConstPool, ConstTag};
pub use decode::{checked_entry_count, max_entries_for_remaining};
pub use encode::{EncodeError, u32_len};
pub use function::{FunctionRecord, FunctionTable};
pub use header::{ENTRY_NONE, FileHeader, HeaderError, MAGIC, VERSION_MAJOR, VERSION_MINOR};
pub use instr::{InstrError, Instruction};
pub use local_layout::{FunctionLocalLayout, LocalLayoutError, LocalLayoutTable, LocalSlotKind};
pub use module::{BytecodeModule, ModuleError};
pub use opcode::{Opcode, OpcodeError};
pub use scalar::{
    PTR_AGG_TAG, PTR_CONST_TAG, PTR_FN_TAG, PTR_LOCAL_TAG, ScalarValue, decode_fn_ptr,
    fn_ptr_from_id, is_fn_ptr,
};
pub use section::{SectionEntry, SectionError, SectionKind, validate_section_table};
pub use stack_effect::{StackEffectError, apply_stack_effect};
pub use stack_flow::{StackFlowError, StackFlowSummary, analyze_stack_cfg, return_stack_depth};
pub use types::{TypeKind, TypeRecord, TypeTable};
pub use verify::{VerifyError, verify};

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn empty_module_round_trip() {
        let module = BytecodeModule::empty();
        let bytes = module.encode().expect("encode");
        let decoded = BytecodeModule::decode(&bytes).expect("decode");
        assert_eq!(decoded.header.section_count, 5);
        assert!(matches!(
            verify(&decoded),
            Err(VerifyError::InvalidEntryFunction)
        ));
    }

    #[test]
    fn reject_bad_magic() {
        let mut bytes = BytecodeModule::empty().encode().expect("encode");
        bytes[0] = b'X';
        let err = BytecodeModule::decode(&bytes).unwrap_err();
        assert!(matches!(err, ModuleError::Header(HeaderError::BadMagic)));
    }

    #[test]
    fn opcode_discriminants_stable() {
        assert_eq!(Opcode::Const.as_u8(), 0);
        assert_eq!(Opcode::LoadLocal.as_u8(), 1);
        assert_eq!(Opcode::Call.as_u8(), 14);
        assert_eq!(Opcode::MakeFnPtr.as_u8(), 45);
        assert_eq!(Opcode::CallIndirect.as_u8(), 46);
    }

    #[test]
    fn instruction_round_trip() {
        let inst = Instruction {
            opcode: Opcode::Add,
            operands: vec![1, 2],
        };
        let bytes = inst.encode();
        let (decoded, end) = Instruction::decode_at(&bytes, 0).expect("decode");
        assert_eq!(end, bytes.len());
        assert_eq!(decoded, inst);
    }
}

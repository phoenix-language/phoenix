//! Phoenix bytecode — `PHX0` encode/decode, opcode definitions, and verification.
//!
//! Portable output of the compiler; consumed by [`phx_vm`] after the verifier pass.
//! Format contract: `docs/design/features/vm-linear.md`.

mod cast;
mod const_pool;
mod local_layout;
mod scalar;
mod function;
mod header;
mod instr;
mod module;
mod opcode;
mod section;
mod stack_effect;
mod types;
mod verify;

pub use cast::{PrimitiveKind, SLOT_KIND_AGG};
pub use local_layout::{FunctionLocalLayout, LocalLayoutError, LocalLayoutTable, LocalSlotKind};
pub use scalar::{ScalarValue, PTR_AGG_TAG, PTR_LOCAL_TAG};
pub use const_pool::{ConstEntry, ConstPool, ConstTag};
pub use function::{FunctionRecord, FunctionTable};
pub use header::{FileHeader, HeaderError, MAGIC, VERSION_MAJOR, VERSION_MINOR};
pub use instr::{InstrError, Instruction};
pub use module::{BytecodeModule, ModuleError};
pub use opcode::{Opcode, OpcodeError};
pub use section::{SectionEntry, SectionError, SectionKind};
pub use stack_effect::{StackEffectError, apply_stack_effect};
pub use types::{TypeKind, TypeRecord, TypeTable};
pub use verify::{VerifyError, verify};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_module_round_trip() {
        let module = BytecodeModule::empty();
        let bytes = module.encode();
        let decoded = BytecodeModule::decode(&bytes).expect("decode");
        assert_eq!(decoded.header.section_count, 5);
        assert!(matches!(
            verify(&decoded),
            Err(VerifyError::InvalidEntryFunction)
        ));
    }

    #[test]
    fn reject_bad_magic() {
        let mut bytes = BytecodeModule::empty().encode();
        bytes[0] = b'X';
        let err = BytecodeModule::decode(&bytes).unwrap_err();
        assert!(matches!(err, ModuleError::Header(HeaderError::BadMagic)));
    }

    #[test]
    fn opcode_discriminants_stable() {
        assert_eq!(Opcode::Const.as_u8(), 0);
        assert_eq!(Opcode::LoadLocal.as_u8(), 1);
        assert_eq!(Opcode::Call.as_u8(), 14);
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

//! Codegen failures.

use phx_bytecode::{EncodeError, InstrError};

/// IR → bytecode lowering failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodegenError {
    /// Section or offset does not fit in `u32`.
    SectionTooLarge {
        /// Section name.
        section: &'static str,
        /// Length or index that overflowed.
        len: usize,
    },
    /// Instruction operand count exceeds wire-format limit.
    InstructionEncode(InstrError),
    /// IR literal index has no entry in the constant pool mapping.
    MissingLiteralIndex {
        /// IR literal index from [`crate::ir::IrInst::Const`] or [`crate::ir::IrInst::MakeStr`].
        literal_index: u32,
    },
    /// IR references a layout type id missing from the module-local type remap.
    MissingTypeId {
        /// Program-global layout type id.
        type_id: u32,
    },
    /// IR call or drop references a definition with no function id mapping.
    MissingCallee {
        /// Raw [`crate::resolver::DefId`] index.
        def_index: u32,
    },
    /// Jump target basic block has no computed code offset.
    InvalidJumpBlock {
        /// Basic block index from IR control flow.
        block: u32,
    },
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SectionTooLarge { section, len } => {
                write!(
                    f,
                    "codegen: section `{section}` size {len} exceeds u32::MAX"
                )
            }
            Self::MissingLiteralIndex { literal_index } => {
                write!(
                    f,
                    "codegen: constant pool has no entry for IR literal index {literal_index}"
                )
            }
            Self::MissingTypeId { type_id } => {
                write!(
                    f,
                    "codegen: module type table has no entry for layout type id {type_id}"
                )
            }
            Self::MissingCallee { def_index } => {
                write!(
                    f,
                    "codegen: no function id mapping for definition {def_index}"
                )
            }
            Self::InvalidJumpBlock { block } => {
                write!(f, "codegen: no code offset for basic block {block}")
            }
            Self::InstructionEncode(err) => write!(f, "codegen: {err}"),
        }
    }
}

impl std::error::Error for CodegenError {}

impl From<EncodeError> for CodegenError {
    fn from(e: EncodeError) -> Self {
        match e {
            EncodeError::SectionTooLarge { section, len } => Self::SectionTooLarge { section, len },
        }
    }
}

impl From<InstrError> for CodegenError {
    fn from(e: InstrError) -> Self {
        Self::InstructionEncode(e)
    }
}

/// Converts `len` to `u32` for codegen tables.
pub fn u32_section(section: &'static str, len: usize) -> Result<u32, CodegenError> {
    u32::try_from(len).map_err(|_| CodegenError::SectionTooLarge { section, len })
}

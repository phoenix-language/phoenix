//! IR → PHX0 bytecode lowering failures.
//!
//! [`CodegenError`] covers the [`codegen`](crate::codegen) pass: section size limits,
//! constant pool and symbol-table lookups, control-flow layout, and instruction encoding.
//! [`crate::compile::CompileError::Codegen`] and [`crate::build::BuildError::Codegen`]
//! wrap these for single-unit and project builds respectively.
//!
//! ## Error taxonomy
//!
//! Variants mirror emission sub-stages in roughly execution order:
//!
//! 1. **Table sizing** — [`CodegenError::SectionTooLarge`]
//! 2. **Pool and symbol lookups** — [`CodegenError::MissingLiteralIndex`],
//!    [`CodegenError::MissingTypeId`], [`CodegenError::MissingCallee`]
//! 3. **Control-flow layout** — [`CodegenError::InvalidJumpBlock`]
//! 4. **Instruction encoding** — [`CodegenError::InstructionEncode`]
//!
//! Lookup variants usually indicate an internal pipeline inconsistency (IR indices out of sync
//! with typeck or lower tables) rather than a user source error; the emitter validates indices
//! before writing PHX0 bytes.

use phx_bytecode::{EncodeError, InstrError};

/// Failure while lowering IR to PHX0 bytecode.
///
/// Returned by [`super::codegen`], [`super::codegen_module`], and helpers in [`super::emit`]
/// when section limits, pool lookups, jump layout, or instruction operands cannot be encoded.
/// Single-file compilation surfaces these as [`crate::compile::CompileError::Codegen`]; project
/// builds use [`crate::build::BuildError::Codegen`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodegenError {
    /// Section or offset does not fit in `u32`.
    ///
    /// Raised when a code section, constant pool, type table, or other on-disk table exceeds
    /// the PHX0 wire-format limit ([`u32::MAX`] entries or bytes as applicable).
    SectionTooLarge {
        /// Section name (for example `"const_pool"`, `"code"`).
        section: &'static str,
        /// Length or index that overflowed.
        len: usize,
    },
    /// Instruction operand count exceeds wire-format limit.
    ///
    /// Wraps [`InstrError`] from [`phx_bytecode`] when an opcode's operand list is too long
    /// or otherwise invalid for encoding.
    InstructionEncode(InstrError),
    /// IR literal index has no entry in the constant pool mapping.
    ///
    /// Emitted when [`super::const_pool::ConstPoolBuilder::pool_index_for_literal`] is called
    /// with an index outside the range registered by [`super::const_pool::ConstPoolBuilder::fill_from_ir`],
    /// or when [`IrInst::Const`](crate::ir::IrInst::Const) / [`IrInst::MakeStr`](crate::ir::IrInst::MakeStr)
    /// references a literal that was never pooled.
    MissingLiteralIndex {
        /// IR literal index from [`crate::ir::IrInst::Const`] or [`crate::ir::IrInst::MakeStr`].
        literal_index: u32,
    },
    /// IR references a layout type id missing from the module-local type remap.
    ///
    /// The emitter maps program-global layout ids from type-check into per-module type table
    /// indices; this error means the IR referenced an id absent from that remap.
    MissingTypeId {
        /// Program-global layout type id.
        type_id: u32,
    },
    /// IR call or drop references a definition with no function id mapping.
    ///
    /// Cross-module and local calls require a stable function id in the module's function table;
    /// this error means lower/codegen did not assign one for the referenced [`DefId`](crate::resolver::DefId).
    MissingCallee {
        /// Raw [`crate::resolver::DefId`] index.
        def_index: u32,
    },
    /// Jump target basic block has no computed code offset.
    ///
    /// Branch and jump instructions need a PC offset for each target block; this error means
    /// layout did not assign an offset to the referenced block (for example unreachable or
    /// unvisited in the flatten pass).
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

/// Converts a section length or index to `u32` for PHX0 on-disk fields.
///
/// Used when recording code offsets, pool indices, and other table sizes during emission.
///
/// # Errors
///
/// Returns [`CodegenError::SectionTooLarge`] when `len` exceeds [`u32::MAX`].
///
/// # Panics
///
/// Never panics; overflow is reported as [`CodegenError::SectionTooLarge`].
pub fn u32_section(section: &'static str, len: usize) -> Result<u32, CodegenError> {
    u32::try_from(len).map_err(|_| CodegenError::SectionTooLarge { section, len })
}

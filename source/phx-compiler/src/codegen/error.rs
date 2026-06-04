//! Codegen failures.

use phx_bytecode::EncodeError;

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

/// Converts `len` to `u32` for codegen tables.
pub fn u32_section(section: &'static str, len: usize) -> Result<u32, CodegenError> {
    u32::try_from(len).map_err(|_| CodegenError::SectionTooLarge { section, len })
}

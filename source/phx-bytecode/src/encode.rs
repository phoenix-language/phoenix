//! PHX0 file encoding errors.

/// Failure while encoding a bytecode module or section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// A section or offset does not fit in `u32`.
    SectionTooLarge {
        /// Section name (e.g. `"code"`).
        section: &'static str,
        /// Byte length or index that overflowed.
        len: usize,
    },
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SectionTooLarge { section, len } => {
                write!(f, "section `{section}` size {len} exceeds u32::MAX")
            }
        }
    }
}

impl std::error::Error for EncodeError {}

/// Converts `len` to `u32` or returns [`EncodeError::SectionTooLarge`].
///
/// # Errors
///
/// Returns [`EncodeError::SectionTooLarge`] when `len` exceeds `u32::MAX`.
pub fn u32_len(section: &'static str, len: usize) -> Result<u32, EncodeError> {
    u32::try_from(len).map_err(|_| EncodeError::SectionTooLarge { section, len })
}

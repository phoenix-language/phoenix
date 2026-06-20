//! PHX0 file encoding errors and length conversion for trusted compiler output.
//!
//! The linker and codegen paths build [`crate::BytecodeModule`] images in memory from verified
//! compiler data. Section offsets and lengths in the PHX0 header and section table are stored as
//! little-endian `u32` values; this module centralizes conversion and reports overflow instead of
//! truncating silently.
//!
//! This module is the **encode** side of the codec split:
//!
//! - **Here** — [`u32_len`] and [`EncodeError`] when a trusted section or offset exceeds `u32::MAX`.
//! - [`crate::decode`] — bounds-check declared entry counts against untrusted payload bytes before
//!   parsing; failures surface as `None` or section `Truncated` errors, not [`EncodeError`].
//!
//! ## Owning passes
//!
//! - **Codegen / linker** — [`crate::BytecodeModule::section_layout`] and
//!   [`crate::BytecodeModule::encode`] call [`u32_len`] for each section offset and length.
//! - **Compiler build** — [`EncodeError`] is mapped into codegen and build error types for
//!   user-facing diagnostics when an image would exceed wire limits.
//!
//! ## In this module
//!
//! - [`EncodeError`] — overflow while sizing or serializing a section.
//! - [`u32_len`] — infallible-length-to-`u32` helper used when writing the section table.

/// Failure while encoding a bytecode module or section.
///
/// Returned when a trusted in-memory section, offset, or entry count cannot be represented in the
/// PHX0 wire format. Unlike decode-time truncation checks in [`crate::decode`], these errors mean
/// the **compiler output** exceeds format limits, not that an input file was malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// A section or offset does not fit in `u32`.
    SectionTooLarge {
        /// Section name (e.g. `"code"`, `"constants_offset"`).
        section: &'static str,
        /// Byte length or index that overflowed `u32::MAX`.
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
/// Use when writing section table entries or recording payload sizes during
/// [`crate::BytecodeModule::encode`]. The `section` label appears in the error message so callers
/// can identify which section overflowed.
///
/// # Errors
///
/// Returns [`EncodeError::SectionTooLarge`] when `len` exceeds [`u32::MAX`].
///
/// # Examples
///
/// ```
/// use phx_bytecode::{EncodeError, u32_len};
///
/// assert_eq!(u32_len("code", 1024).unwrap(), 1024);
///
/// let err = u32_len("code", usize::MAX).unwrap_err();
/// assert!(matches!(err, EncodeError::SectionTooLarge { section: "code", .. }));
/// ```
///
/// # Panics
///
/// Never panics.
pub fn u32_len(section: &'static str, len: usize) -> Result<u32, EncodeError> {
    u32::try_from(len).map_err(|_| EncodeError::SectionTooLarge { section, len })
}

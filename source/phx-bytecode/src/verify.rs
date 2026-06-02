//! Bytecode verifier (MVP stub: header and section bounds only).

use super::header::MAGIC;
use super::module::BytecodeModule;

/// Verifier failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// File too small for header.
    Truncated,
    /// Magic is not `PHX0`.
    BadMagic,
    /// Section table or payload extends past file end.
    SectionOutOfBounds,
    /// MVP flags field must be zero.
    NonZeroFlags,
}

/// Verifies `module` invariants required before execution (MVP subset).
///
/// # Errors
///
/// Returns [`VerifyError`] when header or section layout is invalid.
pub fn verify(module: &BytecodeModule) -> Result<(), VerifyError> {
    let bytes = module.encode();
    if bytes.len() < 24 {
        return Err(VerifyError::Truncated);
    }
    if bytes[0..4] != MAGIC {
        return Err(VerifyError::BadMagic);
    }
    if module.header.flags != 0 {
        return Err(VerifyError::NonZeroFlags);
    }
    let table_end = 24usize.saturating_add(
        usize::try_from(module.header.section_count)
            .unwrap_or(0)
            .saturating_mul(12),
    );
    if bytes.len() < table_end {
        return Err(VerifyError::Truncated);
    }
    for i in 0..module.header.section_count {
        let start = 24 + usize::try_from(i).unwrap_or(0).saturating_mul(12);
        let offset = u32::from_le_bytes([
            bytes[start + 4],
            bytes[start + 5],
            bytes[start + 6],
            bytes[start + 7],
        ]);
        let length = u32::from_le_bytes([
            bytes[start + 8],
            bytes[start + 9],
            bytes[start + 10],
            bytes[start + 11],
        ]);
        let end = u64::from(offset).saturating_add(u64::from(length));
        if end > bytes.len() as u64 {
            return Err(VerifyError::SectionOutOfBounds);
        }
    }
    Ok(())
}

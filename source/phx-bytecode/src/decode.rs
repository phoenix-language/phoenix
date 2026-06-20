//! Decode-time bounds for untrusted PHX0 section payloads.
//!
//! Section decoders read entry counts and fixed-size records from bytes that may come from disk,
//! the network, or other untrusted sources. A hostile or truncated file can declare more entries
//! than the remaining payload can hold; these helpers cap counts **before** indexing so decoders
//! return structured truncation errors instead of panicking or reading past the buffer.
//!
//! This module is the **decode** side of the codec split:
//!
//! - **Here** — validate declared counts against `remaining` bytes using a per-entry minimum size.
//! - [`crate::encode`] — convert trusted in-memory lengths to `u32` wire fields; failures are
//!   [`crate::EncodeError`], not silent truncation.
//!
//! ## Pattern
//!
//! Each section decoder knows the minimum byte size of one entry (for example 4 for a u32 tag,
//! 8 for a type record). Before looping, call [`checked_entry_count`] with the bytes left in the
//! section payload, that minimum, and the count read from the wire. `None` means the declaration
//! cannot fit and the decoder should return its section-specific `Truncated` error.
//!
//! ## Owning passes
//!
//! - **Section decoders** — [`crate::ConstPool`], [`crate::TypeTable`], [`crate::FunctionTable`],
//!   [`crate::LocalLayoutTable`], [`crate::Instruction`], [`crate::PcSpanTable`] call
//!   [`checked_entry_count`] while parsing payloads.
//! - **Verifier** — relies on the same decoders; does not call these helpers directly.
//!
//! ## In this module
//!
//! - [`max_entries_for_remaining`] — upper bound on entries that fit in `remaining` bytes.
//! - [`checked_entry_count`] — returns `declared` when it fits, otherwise `None`.

/// Largest entry count that fits in `remaining` bytes when each entry needs at least `min_entry_size`.
///
/// Computes `remaining / min_entry_size` using saturating division: if `min_entry_size` is zero
/// (misconfiguration) or the division would overflow, returns `0` so callers never treat an
/// unbounded count as valid.
///
/// # Examples
///
/// ```
/// use phx_bytecode::max_entries_for_remaining;
///
/// assert_eq!(max_entries_for_remaining(12, 4), 3);
/// assert_eq!(max_entries_for_remaining(11, 4), 2);
/// assert_eq!(max_entries_for_remaining(0, 4), 0);
/// assert_eq!(max_entries_for_remaining(100, 0), 0);
/// ```
///
/// # Panics
///
/// Never panics.
#[must_use]
pub const fn max_entries_for_remaining(remaining: usize, min_entry_size: usize) -> usize {
    match remaining.checked_div(min_entry_size) {
        Some(n) => n,
        None => 0,
    }
}

/// Returns `declared` when it fits in `remaining` bytes; otherwise `None`.
///
/// Compares `declared` against [`max_entries_for_remaining`] so decoders can reject wire counts
/// that would require more bytes than the section payload contains. A `None` result indicates
/// truncation or a corrupt count — map it to the section's `Truncated` error variant rather than
/// looping past the buffer end.
///
/// # Examples
///
/// ```
/// use phx_bytecode::checked_entry_count;
///
/// // Three 4-byte records fit exactly in 12 bytes.
/// assert_eq!(checked_entry_count(12, 4, 3), Some(3));
///
/// // Declaring four records needs 16 bytes; payload has only 12.
/// assert_eq!(checked_entry_count(12, 4, 4), None);
///
/// // Oversized declared counts are rejected without wrapping.
/// assert_eq!(checked_entry_count(8, 4, usize::MAX), None);
/// ```
///
/// # Panics
///
/// Never panics.
#[must_use]
pub fn checked_entry_count(
    remaining: usize,
    min_entry_size: usize,
    declared: usize,
) -> Option<usize> {
    let max = max_entries_for_remaining(remaining, min_entry_size);
    (declared <= max).then_some(declared)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn max_entries_zero_remaining() {
        assert_eq!(max_entries_for_remaining(0, 4), 0);
    }

    #[test]
    fn max_entries_zero_min_size() {
        assert_eq!(max_entries_for_remaining(100, 0), 0);
    }

    #[test]
    fn max_entries_exact_fit() {
        assert_eq!(max_entries_for_remaining(12, 4), 3);
    }

    #[test]
    fn checked_entry_count_exact_fit() {
        assert_eq!(checked_entry_count(12, 4, 3), Some(3));
    }

    #[test]
    fn checked_entry_count_off_by_one() {
        assert_eq!(checked_entry_count(12, 4, 4), None);
    }

    #[test]
    fn checked_entry_count_huge_declared() {
        assert_eq!(checked_entry_count(8, 4, 0xFFFF_FFFF), None);
    }
}

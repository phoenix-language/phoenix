//! Decode-time bounds for untrusted PHX0 section counts.

/// Largest entry count that fits in `remaining` bytes when each entry needs at least `min_entry_size`.
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

/// Returns `declared` when it fits in `remaining`; otherwise `None`.
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

//! Static explanations for diagnostic codes (`phx explain`).
//!
//! Re-exported at the crate root as [`crate::explain_code`] (alias of [`lookup`]).
//! The underlying registry lives in [`crate::render::explain_code`].

use crate::render::explain_code;

/// Returns a human-readable explanation for `code` (e.g. `E2001`).
///
/// Delegates to [`crate::render::explain_code`]. Returns `None` for unknown codes.
#[must_use]
pub fn lookup(code: &str) -> Option<&'static str> {
    explain_code(code)
}

/// Normalizes user input to canonical `E####` / `W####` form.
///
/// Accepts case-insensitive five-character codes (`e2001`, `W3001`). Returns `None` when the
/// input is not exactly one letter (`E` or `W`) followed by four ASCII digits.
#[must_use]
pub fn normalize_code(input: &str) -> Option<String> {
    let upper = input.to_ascii_uppercase();
    if upper.len() == 5
        && matches!(upper.as_bytes()[0], b'E' | b'W')
        && upper[1..].chars().all(|c| c.is_ascii_digit())
    {
        return Some(upper);
    }
    None
}

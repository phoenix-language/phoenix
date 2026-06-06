//! Static explanations for diagnostic codes (`phx explain`).

use crate::render::explain_code;

/// Returns a human-readable explanation for `code` (e.g. `E2001`).
#[must_use]
pub fn lookup(code: &str) -> Option<&'static str> {
    explain_code(code)
}

/// Normalizes user input (`e2001`, `E2001`) to canonical `E####` form when valid.
#[must_use]
pub fn normalize_code(input: &str) -> Option<String> {
    let upper = input.to_ascii_uppercase();
    if upper.len() == 5 && upper.starts_with('E') && upper[1..].chars().all(|c| c.is_ascii_digit())
    {
        return Some(upper);
    }
    None
}

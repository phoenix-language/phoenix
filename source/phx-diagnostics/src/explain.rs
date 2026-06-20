//! Static explanations for diagnostic codes (`phx explain`).
//!
//! This module is the **public facade** for looking up long-form explanation text keyed by stable
//! [`DiagnosticCode`](crate::DiagnosticCode) labels (`E####` for errors, `W####` for warnings).
//! The CLI command `phx explain E2001` and embedders call [`crate::explain_code`] (a re-export of
//! [`lookup`]) after normalizing user input with [`normalize_code`].
//!
//! ## Division of responsibility
//!
//! | Location | Role |
//! |----------|------|
//! | [`crate::render::explain_code`] | Canonical static registry — one match arm per registered code |
//! | `explain` (this module) | Public lookup wrapper and input normalization |
//! | [`crate::explain_coverage`] | Drift tests ensuring every pass registers explain text |
//!
//! When adding a new diagnostic code, add prose to [`crate::render::explain_code`] first, then
//! extend the drift tests in the owning pass (type-check codes are also checked via
//! [`crate::type_error_registry`]). This module does not hold the table itself.
//!
//! ## CLI flow
//!
//! ```text
//! user code string
//!       │
//!       ▼
//! normalize_code ──► None ──► usage error (invalid format)
//!       │
//!       ▼
//! explain_code / lookup ──► None ──► usage error (unknown code)
//!       │
//!       ▼
//! print "{code}: {explanation}"
//! ```
//!
//! [`normalize_code`] accepts case-insensitive five-character codes (`e2001`, `W3001`). Codes with
//! wrong length, non-digit suffixes, or a leading letter other than `E`/`W` are rejected before
//! lookup.

use crate::render::explain_code;

/// Returns a human-readable explanation for `code`.
///
/// Delegates to the static registry in [`crate::render::explain_code`]. The input should already
/// be normalized (uppercase `E####` / `W####`); callers that accept raw CLI input should run
/// [`normalize_code`] first.
///
/// Returns `None` when no explanation is registered for `code`. A registered code always returns
/// `Some` with static string data (no allocation).
///
/// # Examples
///
/// ```
/// use phx_diagnostics::explain_code;
///
/// assert!(explain_code("E2001").is_some());
/// assert!(explain_code("W3001").is_some());
/// assert!(explain_code("E9999").is_none());
/// ```
///
/// # Panics
///
/// Never panics on any input string.
#[must_use]
pub fn lookup(code: &str) -> Option<&'static str> {
    explain_code(code)
}

/// Normalizes user input to canonical `E####` / `W####` form.
///
/// Accepts exactly five characters: one ASCII letter (`E` or `W`, any case) followed by four
/// ASCII digits. The returned string is always uppercase.
///
/// Returns `None` when:
///
/// - the input length is not 5;
/// - the first character is not `E` or `W` (after uppercasing);
/// - any of the last four characters is not an ASCII digit.
///
/// Leading zeros are preserved (`e0001` → `E0001`). Shorter or longer numeric suffixes are
/// rejected (`E200`, `E20001`).
///
/// # Examples
///
/// ```
/// use phx_diagnostics::normalize_code;
///
/// assert_eq!(normalize_code("e2001"), Some("E2001".to_owned()));
/// assert_eq!(normalize_code("w3002"), Some("W3002".to_owned()));
/// assert_eq!(normalize_code("E2001"), Some("E2001".to_owned()));
/// assert_eq!(normalize_code("E200"), None);
/// assert_eq!(normalize_code("X2001"), None);
/// assert_eq!(normalize_code("E20a1"), None);
/// ```
///
/// # Panics
///
/// Never panics on any input string.
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

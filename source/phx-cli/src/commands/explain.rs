//! `phx explain` handler — print documentation for a diagnostic code.
//!
//! This module is the entry point for [`run_explain`]. It normalizes a user-
//! supplied error or warning code, looks up static explanation text from
//! `phx-diagnostics`, and prints a single line to stderr. No compiler or VM
//! pipeline runs.
//!
//! ```text
//! explain <code>
//!       │
//!       ▼
//! normalize_code (E#### | W####)
//!       │
//!       ▼
//! explain_code ──► eprintln! ──► CliExit::Ok
//! ```
//!
//! ## Project vs standalone
//!
//! Not applicable — this command takes only a diagnostic code string and does
//! not touch the filesystem or project layout.
//!
//! ## Compiler passes invoked
//!
//! None. Lookup is a static table in [`phx_diagnostics::explain_code`].
//!
//! ## Exit codes
//!
//! | Outcome | [`crate::exit::CliExit`] |
//! | --- | --- |
//! | Known code with explanation | [`CliExit::Ok`] |
//! | Invalid code format or unknown code | [`CliExit::Usage`] |
//!
//! ## Public entry points
//!
//! - [`run_explain`] — run `phx explain <code>`.

use phx_diagnostics::{explain_code, normalize_code};

use crate::color::ColorChoice;
use crate::exit::CliExit;
use crate::report::Reporter;

/// Runs `phx explain <code>` — print the static explanation for a diagnostic code.
///
/// Accepts codes in the form `E####` or `W####` (case and leading zeros normalized
/// by [`normalize_code`]). On success, prints `{code}: {explanation}` to stderr
/// and returns [`CliExit::Ok`].
///
/// # Errors
///
/// Returns a non-[`CliExit::Ok`] variant instead of panicking:
///
/// - [`CliExit::Usage`] — code string is not a valid diagnostic id, or no
///   explanation is registered for the normalized code.
///
/// # Panics
///
/// Never panics on user input.
pub fn run_explain(code: String, color: ColorChoice) -> CliExit {
    let style = crate::color::diagnostic_style(color);
    let reporter = Reporter::new(&style);

    let Some(normalized) = normalize_code(&code) else {
        reporter.usage_error(&format!(
            "invalid diagnostic code '{code}' (expected E#### or W####)"
        ));
        return CliExit::Usage;
    };

    match explain_code(&normalized) {
        Some(text) => {
            eprintln!("{normalized}: {text}");
            CliExit::Ok
        }
        None => {
            reporter.usage_error(&format!("no explanation available for {normalized}"));
            CliExit::Usage
        }
    }
}

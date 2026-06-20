//! Stable diagnostic codes for tooling and long-lived references.
//!
//! [`DiagnosticCode`] is the machine-stable half of a Phoenix diagnostic. User-facing **message
//! strings** may change between compiler versions; **codes** (`E0101`, `W0203`, …) should not, so
//! IDEs, CI filters, and `phx explain` can key off them without brittle text matching.
//!
//! ## Role in the pipeline
//!
//! Each pass-specific error enum implements a `code()` method that returns a [`DiagnosticCode`]:
//!
//! | Pass | Code range | Registry |
//! |------|------------|----------|
//! | Lex | E0001–E0009 | [`crate::LexError::code`] |
//! | Resolve | E1xxx | [`crate::ResolveError::code`] |
//! | Parse | E3xxx | [`crate::ParseError::code`] |
//! | Type-check | E2001–E2046 | generated [`crate::type_error_registry`] |
//! | Lower / IR | E4xxx | [`crate::LowerError::code`], [`crate::IrError::code`] |
//!
//! Formatters prepend the code to rendered output; [`crate::explain_code`] maps normalized codes
//! to static explanation text for `phx explain E0101`.
//!
//! ## Invariants
//!
//! - Labels are `&'static str` literals defined alongside their error enum — never allocated at
//!   runtime and never derived from user input.
//! - Every registered code should have a matching entry in [`crate::explain_code`]; see
//!   `explain_coverage` tests in this crate.
//!
//! [`crate::LexError::code`]: crate::LexError::code
//! [`crate::ResolveError::code`]: crate::ResolveError::code
//! [`crate::ParseError::code`]: crate::ParseError::code
//! [`crate::LowerError::code`]: crate::LowerError::code
//! [`crate::IrError::code`]: crate::IrError::code

use core::fmt;

/// A stable error or warning label (for example `E0101` or `W0203`).
///
/// Display via [`DiagnosticCode`] or [`fmt::Display`] in headers and `phx explain` lookup keys.
/// Message prose lives on the error enum's [`Display`] impl or in [`crate::format`] helpers — not
/// in this type.
///
/// Construct with [`DiagnosticCode::new`] using the same literal returned from the owning error's
/// `code()` method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagnosticCode(pub &'static str);

impl DiagnosticCode {
    /// Wraps a static code label.
    ///
    /// # Panics
    ///
    /// Never panics. Callers should pass only compile-time string literals from the diagnostic
    /// registries; empty labels are discouraged but not rejected here.
    #[must_use]
    pub const fn new(label: &'static str) -> Self {
        Self(label)
    }
}

impl fmt::Display for DiagnosticCode {
    /// Writes the code label verbatim (for example `E2042`).
    ///
    /// # Panics
    ///
    /// Never panics.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

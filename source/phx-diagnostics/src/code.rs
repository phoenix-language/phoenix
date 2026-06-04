//! Stable diagnostic codes for tooling (`--explain`, LSP) without freezing message text.

use core::fmt;

/// A stable error or warning code (e.g. `E0101`). Message strings may change; codes should not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagnosticCode(pub &'static str);

impl DiagnosticCode {
    /// Creates a code label.
    #[must_use]
    pub const fn new(label: &'static str) -> Self {
        Self(label)
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

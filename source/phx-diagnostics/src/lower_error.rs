//! IR lowering failure types (internal compiler invariant violations).

use core::fmt;

use crate::LocatedError;
use crate::Span;
use crate::code::DiagnosticCode;

/// A lowering error when typed AST and resolution tables are inconsistent.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LowerError {
    /// Function or method call with no resolved callee at the use site.
    UnresolvedCallee {
        /// Call expression span.
        span: Span,
    },
}

impl LowerError {
    /// Stable diagnostic code for this error.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::UnresolvedCallee { .. } => DiagnosticCode::new("E4001"),
        }
    }

    /// Source span when available.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        match self {
            Self::UnresolvedCallee { span } => Some(*span),
        }
    }
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedCallee { .. } => {
                f.write_str("internal error: unresolved call target during lowering")
            }
        }
    }
}

impl std::error::Error for LowerError {}

/// Result of lowering when errors may be collected.
pub type LowerResult<T> = Result<T, LowerBag>;

/// Collected lowering diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LowerBag {
    errors: Vec<LocatedError<LowerError>>,
}

impl LowerBag {
    /// Creates an empty bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an error for `module`.
    pub fn push(&mut self, module: u32, error: LowerError) {
        self.errors.push(LocatedError::new(module, error));
    }

    /// Returns true when at least one error was recorded.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Borrows collected errors.
    #[must_use]
    pub fn errors(&self) -> &[LocatedError<LowerError>] {
        &self.errors
    }

    /// Consumes the bag into a flat error list.
    #[must_use]
    pub fn into_errors(self) -> Vec<LocatedError<LowerError>> {
        self.errors
    }
}

impl std::fmt::Display for LowerBag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, located) in self.errors.iter().enumerate() {
            if i > 0 {
                f.write_str("\n---\n")?;
            }
            write!(f, "{}", located.error)?;
        }
        Ok(())
    }
}

impl std::error::Error for LowerBag {}

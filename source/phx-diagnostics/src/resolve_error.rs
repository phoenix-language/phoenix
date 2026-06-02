//! Name-resolution failure types.
//!
//! Collected in [`DiagnosticBag`] during [`phx_compiler::resolve`] (imports, duplicates, `main`).

use core::fmt;

use crate::Span;

/// Reason a `main` function fails the MVP entry contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidMainReason {
    /// `main` has one or more parameters.
    HasParameters,
    /// Return type is present and is not unit `()`.
    NonUnitReturn,
}

impl fmt::Display for InvalidMainReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HasParameters => f.write_str("main must have no parameters"),
            Self::NonUnitReturn => f.write_str("main must return unit type `()`"),
        }
    }
}

/// A resolve error produced while binding names in the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolveError {
    /// Value identifier not found in scope.
    UnresolvedIdent {
        /// Interned name (display via interner).
        symbol_index: u32,
        /// Use site span.
        span: Span,
    },
    /// Type name not found in scope.
    UnresolvedType {
        /// Interned name index.
        symbol_index: u32,
        /// Use site span.
        span: Span,
    },
    /// Name defined more than once in the same scope.
    DuplicateDefinition {
        /// Interned name index.
        symbol_index: u32,
        /// Span of the earlier definition.
        first_span: Span,
        /// Span of the duplicate.
        span: Span,
    },
    /// `#import` is not supported until multi-file loading exists.
    ImportNotSupported {
        /// Span of the import directive.
        span: Span,
    },
    /// No `main` function in the compilation unit.
    MissingMain,
    /// `main` exists but does not match the MVP signature.
    InvalidMainSignature {
        /// Span of the `main` function name or signature.
        span: Span,
        /// What failed.
        reason: InvalidMainReason,
    },
}

impl ResolveError {
    /// Returns the primary span for this error, if any.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        match self {
            Self::UnresolvedIdent { span, .. }
            | Self::UnresolvedType { span, .. }
            | Self::DuplicateDefinition { span, .. }
            | Self::ImportNotSupported { span }
            | Self::InvalidMainSignature { span, .. } => Some(*span),
            Self::MissingMain => None,
        }
    }
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedIdent { symbol_index, .. } => {
                write!(f, "unresolved identifier (sym#{symbol_index})")
            }
            Self::UnresolvedType { symbol_index, .. } => {
                write!(f, "unresolved type (sym#{symbol_index})")
            }
            Self::DuplicateDefinition { symbol_index, .. } => {
                write!(f, "duplicate definition of sym#{symbol_index}")
            }
            Self::ImportNotSupported { .. } => {
                f.write_str("#import is not supported yet (multi-file modules are not implemented)")
            }
            Self::MissingMain => f.write_str("missing entry function `main`"),
            Self::InvalidMainSignature { reason, .. } => {
                write!(f, "invalid `main` signature: {reason}")
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// Result of a resolve pass that may collect multiple errors.
pub type ResolveResult<T> = Result<T, DiagnosticBag>;

/// Collected resolve diagnostics; resolution may continue after non-fatal errors.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiagnosticBag {
    errors: Vec<ResolveError>,
}

impl DiagnosticBag {
    /// Creates an empty bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an error.
    pub fn push(&mut self, error: ResolveError) {
        self.errors.push(error);
    }

    /// Returns `true` if any errors were recorded.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Borrows collected errors.
    #[must_use]
    pub fn errors(&self) -> &[ResolveError] {
        &self.errors
    }

    /// Consumes the bag and returns errors, for formatting.
    #[must_use]
    pub fn into_errors(self) -> Vec<ResolveError> {
        self.errors
    }
}

impl fmt::Display for DiagnosticBag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, e) in self.errors.iter().enumerate() {
            if i > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{e}")?;
        }
        Ok(())
    }
}

impl std::error::Error for DiagnosticBag {}

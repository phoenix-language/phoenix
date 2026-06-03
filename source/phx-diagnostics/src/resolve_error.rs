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
    /// `#import` used without a module root (single-file `compile_source` / `phx check` with no `--module-src`).
    ImportNotSupported {
        /// Span of the import directive.
        span: Span,
    },
    /// Module file could not be found on disk.
    ModuleNotFound {
        /// Import or reference span.
        span: Span,
        /// Logical module path.
        path: String,
    },
    /// Failed to read a module file.
    ModuleIo {
        /// Related span.
        span: Span,
        /// Filesystem path.
        path: String,
        /// OS error message.
        message: String,
    },
    /// Module source failed to parse.
    ModuleParse {
        /// Related span.
        span: Span,
        /// Filesystem path.
        path: String,
        /// Parse error message.
        message: String,
    },
    /// Circular `#import` dependency.
    CircularImport {
        /// Span (entry or import).
        span: Span,
        /// Human-readable cycle description.
        cycle: String,
    },
    /// Imported symbol is not `pub`.
    ImportNotExported {
        /// Import span.
        span: Span,
        /// Symbol name.
        name: String,
    },
    /// Symbol not found in target module exports.
    ImportNotFound {
        /// Import span.
        span: Span,
        /// Symbol name.
        name: String,
        /// Target module path.
        module: String,
    },
    /// Duplicate name introduced by imports.
    DuplicateImport {
        /// Import span.
        span: Span,
        /// Conflicting symbol.
        name: String,
    },
    /// `main` defined outside the entry module.
    MainNotInEntry {
        /// `main` span.
        span: Span,
        /// Module where `main` was found.
        module: String,
    },
    /// No `main` function in the compilation unit.
    MissingMain,
    /// `main` is not allowed in a `lib` package.
    MainForbiddenInLib {
        /// `main` definition span.
        span: Span,
        /// Module path.
        module: String,
    },
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
            | Self::ModuleNotFound { span, .. }
            | Self::ModuleIo { span, .. }
            | Self::ModuleParse { span, .. }
            | Self::CircularImport { span, .. }
            | Self::ImportNotExported { span, .. }
            | Self::ImportNotFound { span, .. }
            | Self::DuplicateImport { span, .. }
            | Self::MainNotInEntry { span, .. }
            | Self::InvalidMainSignature { span, .. }
            | Self::MainForbiddenInLib { span, .. } => Some(*span),
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
                f.write_str(
                    "#import requires a module root; use `phx check --module-src <dir> <file>` or `phx build` from a project with `phoenix.toml`",
                )
            }
            Self::ModuleNotFound { path, .. } => write!(f, "module not found: `{path}`"),
            Self::ModuleIo { path, message, .. } => {
                write!(f, "failed to read module `{path}`: {message}")
            }
            Self::ModuleParse { path, message, .. } => {
                write!(f, "failed to parse module `{path}`: {message}")
            }
            Self::CircularImport { cycle, .. } => write!(f, "circular module import: {cycle}"),
            Self::ImportNotExported { name, .. } => {
                write!(
                    f,
                    "`{name}` is not exported (add `pub` or import something else)"
                )
            }
            Self::ImportNotFound { name, module, .. } => {
                write!(f, "symbol `{name}` not found in module `{module}`")
            }
            Self::DuplicateImport { name, .. } => {
                write!(f, "duplicate import: `{name}`")
            }
            Self::MainNotInEntry { module, .. } => {
                write!(
                    f,
                    "`main` must be defined in the entry module, not in `{module}`"
                )
            }
            Self::MissingMain => f.write_str("missing entry function `main`"),
            Self::MainForbiddenInLib { module, .. } => {
                write!(
                    f,
                    "`main` is not allowed in library package module `{module}`"
                )
            }
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

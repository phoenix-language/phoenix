//! IR lowering failure types (internal compiler invariant violations).
//!
//! These errors are **not** produced by invalid Phoenix source. They indicate that typed AST,
//! resolution tables, and bytecode layout metadata were inconsistent when
//! [`phx_compiler::lower::func::lower_functions`] walked a function body through
//! [`LowerCtx`](phx_compiler::lower::ctx::LowerCtx).
//!
//! ## User errors vs internal errors
//!
//! Earlier pipeline stages report user-facing diagnostics — for example
//! [`TypeCheckError`](crate::type_error::TypeCheckError) for type mismatches and
//! [`ResolveError`](crate::resolve_error::ResolveError) for unresolved names. A well-typed,
//! resolved program should never hit a [`LowerError`]. When one is recorded, the driver surfaces
//! it as an internal compiler error (`E4001` / `E4002`) with a report prompt rather than a
//! source fix hint.
//!
//! ## Owning pass
//!
//! | Item | Producer |
//! | --- | --- |
//! | [`LowerError`] | [`LowerCtx`](phx_compiler::lower::ctx::LowerCtx) during expression/statement lowering |
//! | [`LowerBag`] | [`lower_functions`](phx_compiler::lower::func::lower_functions) — one bag per module lowering |
//!
//! Lowering aborts the current function on the first error; the bag may hold multiple errors if
//! several functions fail before the driver stops.

use core::fmt;

use crate::LocatedError;
use crate::Span;
use crate::code::DiagnosticCode;

/// A lowering error when typed AST and resolution tables are inconsistent.
///
/// Each variant corresponds to an invariant that [`LowerCtx`](phx_compiler::lower::ctx::LowerCtx)
/// expects after successful type checking and layout construction. None of these can be triggered
/// directly by Phoenix source syntax or semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LowerError {
    /// Function or method call with no resolved callee at the use site.
    ///
    /// Type checking should have rejected non-callable expressions and resolution should have
    /// bound every call target before lowering reaches the call site.
    UnresolvedCallee {
        /// Call expression span.
        span: Span,
    },
    /// Emission targeted a basic block index that does not exist.
    ///
    /// The lowering driver only creates blocks through [`LowerCtx`](phx_compiler::lower::ctx::LowerCtx);
    /// an out-of-range index means the CFG builder and emitter disagree.
    InvalidBlockIndex {
        /// Block index passed to `LowerCtx::emit` or `LowerCtx::set_current` in the lowering driver.
        block: u32,
    },
    /// A lowering table exceeded representable `u32` indices.
    ///
    /// Raised when a pool or index (constants, basic blocks, functions) would overflow the
    /// bytecode module format's 32-bit limits.
    LimitExceeded {
        /// Table name (for example `"const_pool"`, `"basic_blocks"`, `"functions"`).
        item: &'static str,
        /// Length or index that overflowed.
        len: usize,
    },
    /// Missing layout metadata while lowering `?` with `From` error conversion.
    ///
    /// The `?` operator needs the enclosing function's `Result` return layout and the `From`
    /// conversion target; type checking should have populated both before lowering.
    MissingTryConvertLayout {
        /// Invariant detail (for example `"return Result type"`).
        detail: &'static str,
    },
    /// Expression id in the function's typeck range has no entry in `TypedProgram::expr_types`.
    ///
    /// Lowering walks expressions in the same order type checking assigned ids; a gap means the
    /// typed program and AST traversal diverged.
    MissingExprType {
        /// Raw expression id index assigned during type checking.
        expr_id: u32,
        /// Lowering site when the missing type was discovered.
        span: Span,
    },
    /// Lowering consumed a different number of expression ids than typeck assigned.
    ///
    /// After lowering a function body, the expression cursor must equal
    /// [`FunctionLayout::expr_end`](phx_compiler::typeck::FunctionLayout::expr_end). Drift means
    /// some AST nodes were skipped or duplicated during emission.
    ExprCursorDrift {
        /// Expected cursor (`FunctionLayout::expr_end`).
        expected: u32,
        /// Cursor after lowering the function body.
        found: u32,
        /// Lowering site when drift was detected.
        span: Span,
    },
    /// Missing bytecode layout metadata (`type_id`, struct field index, …).
    ///
    /// Named types, enum variants, and struct fields need precomputed layout indices from the
    /// type-check layout phase; lowering should never encounter a hole in those tables.
    MissingLayoutMetadata {
        /// Invariant detail (for example `"bytecode type id for named type"`).
        detail: &'static str,
        /// Lowering site when the missing metadata was discovered.
        span: Span,
    },
}

impl LowerError {
    /// Stable diagnostic code for this error.
    ///
    /// [`LowerError::UnresolvedCallee`] maps to `E4001`; all other variants map to `E4002`.
    /// Both codes render as internal compiler errors.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::UnresolvedCallee { .. } => DiagnosticCode::new("E4001"),
            Self::InvalidBlockIndex { .. }
            | Self::LimitExceeded { .. }
            | Self::MissingTryConvertLayout { .. }
            | Self::MissingExprType { .. }
            | Self::ExprCursorDrift { .. }
            | Self::MissingLayoutMetadata { .. } => DiagnosticCode::new("E4002"),
        }
    }

    /// Source span when available.
    ///
    /// Structural and table-overflow failures have no single source span; expression-linked
    /// variants return the lowering site where the inconsistency was detected.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        match self {
            Self::UnresolvedCallee { span }
            | Self::MissingExprType { span, .. }
            | Self::ExprCursorDrift { span, .. }
            | Self::MissingLayoutMetadata { span, .. } => Some(*span),
            Self::InvalidBlockIndex { .. }
            | Self::LimitExceeded { .. }
            | Self::MissingTryConvertLayout { .. } => None,
        }
    }
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedCallee { .. } => {
                f.write_str("internal error: unresolved call target during lowering")
            }
            Self::InvalidBlockIndex { block } => {
                write!(
                    f,
                    "internal error: invalid basic block index {block} during lowering"
                )
            }
            Self::LimitExceeded { item, len } => {
                write!(
                    f,
                    "internal error: lowering table `{item}` size {len} exceeds u32::MAX"
                )
            }
            Self::MissingTryConvertLayout { detail } => {
                write!(
                    f,
                    "internal error: missing {detail} for `?` conversion during lowering"
                )
            }
            Self::MissingExprType { expr_id, .. } => {
                write!(
                    f,
                    "internal error: missing expression type for id {expr_id} during lowering"
                )
            }
            Self::ExprCursorDrift {
                expected, found, ..
            } => {
                write!(
                    f,
                    "internal error: expression cursor drift during lowering (expected {expected}, found {found})"
                )
            }
            Self::MissingLayoutMetadata { detail, .. } => {
                write!(f, "internal error: missing {detail} during lowering")
            }
        }
    }
}

impl std::error::Error for LowerError {}

/// Result of lowering when errors may be collected.
///
/// Success carries lowered IR; failure is always a [`LowerBag`] of internal invariant violations.
///
/// # Errors
///
/// The `Err` branch is a [`LowerBag`] when lowering recorded one or more
/// [`LowerError`] invariant violations.
pub type LowerResult<T> = Result<T, LowerBag>;

/// Collected lowering diagnostics.
///
/// [`lower_functions`](phx_compiler::lower::func::lower_functions) appends one
/// [`LocatedError`] per failure, keyed by the owning module id. The bag is returned when lowering
/// cannot produce a complete [`IrModule`](phx_compiler::ir::IrModule).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LowerBag {
    errors: Vec<LocatedError<LowerError>>,
}

impl LowerBag {
    /// Creates an empty bag.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an error for `module`.
    ///
    /// `module` is the compilation-unit module index from
    /// [`ResolvedProgram`](phx_compiler::resolver::ResolvedProgram), not a source file path.
    ///
    /// # Panics
    ///
    /// Never panics.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_block_index_display_and_code() {
        let err = LowerError::InvalidBlockIndex { block: 99 };
        assert_eq!(err.code(), DiagnosticCode::new("E4002"));
        assert!(err.span().is_none());
        assert!(err.to_string().contains("block index 99"), "{}", err);
    }

    #[test]
    fn limit_exceeded_display_and_code() {
        let err = LowerError::LimitExceeded {
            item: "basic_blocks",
            len: 1_000,
        };
        assert_eq!(err.code(), DiagnosticCode::new("E4002"));
        assert!(err.span().is_none());
        assert!(err.to_string().contains("basic_blocks"), "{}", err);
        assert!(err.to_string().contains("1000"), "{}", err);
    }

    #[test]
    fn missing_try_convert_layout_display_and_code() {
        let err = LowerError::MissingTryConvertLayout {
            detail: "return Result type",
        };
        assert_eq!(err.code(), DiagnosticCode::new("E4002"));
        assert!(err.span().is_none());
        assert!(err.to_string().contains("return Result type"), "{}", err);
    }

    #[test]
    fn missing_expr_type_display_and_code() {
        let err = LowerError::MissingExprType {
            expr_id: 7,
            span: Span::new(0, 0),
        };
        assert_eq!(err.code(), DiagnosticCode::new("E4002"));
        assert!(err.span().is_some());
        assert!(err.to_string().contains("id 7"), "{}", err);
    }

    #[test]
    fn expr_cursor_drift_display_and_code() {
        let err = LowerError::ExprCursorDrift {
            expected: 10,
            found: 12,
            span: Span::new(0, 0),
        };
        assert_eq!(err.code(), DiagnosticCode::new("E4002"));
        assert!(err.span().is_some());
        assert!(err.to_string().contains("expected 10"), "{}", err);
        assert!(err.to_string().contains("found 12"), "{}", err);
    }

    #[test]
    fn missing_layout_metadata_display_and_code() {
        let err = LowerError::MissingLayoutMetadata {
            detail: "bytecode type id for named type",
            span: Span::new(0, 0),
        };
        assert_eq!(err.code(), DiagnosticCode::new("E4002"));
        assert!(err.span().is_some());
        assert!(
            err.to_string().contains("bytecode type id for named type"),
            "{}",
            err
        );
    }
}

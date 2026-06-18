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
    /// Emission targeted a basic block index that does not exist.
    InvalidBlockIndex {
        /// Block index passed to `LowerCtx::emit` or `LowerCtx::set_current` in the lowering driver.
        block: u32,
    },
    /// A lowering table exceeded representable `u32` indices.
    LimitExceeded {
        /// Table name (for example `"const_pool"`, `"basic_blocks"`, `"functions"`).
        item: &'static str,
        /// Length or index that overflowed.
        len: usize,
    },
    /// Missing layout metadata while lowering `?` with `From` error conversion.
    MissingTryConvertLayout {
        /// Invariant detail (for example `"return Result type"`).
        detail: &'static str,
    },
    /// Expression id in the function's typeck range has no entry in `TypedProgram::expr_types`.
    MissingExprType {
        /// Raw expression id index assigned during type checking.
        expr_id: u32,
        /// Lowering site when the missing type was discovered.
        span: Span,
    },
    /// Lowering consumed a different number of expression ids than typeck assigned.
    ExprCursorDrift {
        /// Expected cursor (`FunctionLayout::expr_end`).
        expected: u32,
        /// Cursor after lowering the function body.
        found: u32,
        /// Lowering site when drift was detected.
        span: Span,
    },
    /// Missing bytecode layout metadata (`type_id`, struct field index, …).
    MissingLayoutMetadata {
        /// Invariant detail (for example `"bytecode type id for named type"`).
        detail: &'static str,
        /// Lowering site when the missing metadata was discovered.
        span: Span,
    },
}

impl LowerError {
    /// Stable diagnostic code for this error.
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

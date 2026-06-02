//! Type-checking failure types.
//!
//! Collected in [`TypeCheckBag`] during [`phx_compiler::type_check`].

use core::fmt;

use crate::Span;

/// A type-check error produced while analyzing the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TypeCheckError {
    /// Expression or binding type does not match expectation.
    Mismatch {
        /// Short description of expected type.
        expected: String,
        /// Short description of found type.
        found: String,
        /// Primary span.
        span: Span,
    },
    /// Could not resolve a type name to a definition.
    UnknownType {
        /// Interned name index.
        symbol_index: u32,
        /// Use site span.
        span: Span,
    },
    /// Call argument count does not match parameters.
    ArityMismatch {
        /// Expected parameter count.
        expected: usize,
        /// Found argument count.
        found: usize,
        /// Call site span.
        span: Span,
    },
    /// Expression is not callable.
    NotCallable {
        /// Type description.
        found: String,
        /// Call site span.
        span: Span,
    },
    /// No method with this name on the receiver type.
    UnresolvedMethod {
        /// Receiver type description.
        receiver: String,
        /// Method name index.
        method_index: u32,
        /// Call site span.
        span: Span,
    },
    /// Multiple trait impls provide the same method name.
    AmbiguousMethod {
        /// Receiver type description.
        receiver: String,
        /// Method name index.
        method_index: u32,
        /// Call site span.
        span: Span,
    },
    /// `if` or `match` arms do not unify to one type.
    NonUnifyingBranches {
        /// Branch span.
        span: Span,
    },
    /// Explicit cast is not allowed between these types.
    InvalidCast {
        /// Source type description.
        from: String,
        /// Target type description.
        to: String,
        /// Cast span.
        span: Span,
    },
    /// Operator cannot be applied to these operand types.
    InvalidOperator {
        /// Operator name.
        op: &'static str,
        /// Span of the operation.
        span: Span,
    },
    /// Post-MVP language or std feature used in MVP build.
    UnsupportedFeature {
        /// Short feature name for the diagnostic.
        feature: &'static str,
        /// Use site span.
        span: Span,
    },
    /// Use of a binding after it was moved.
    UseAfterMove {
        /// Variable name index.
        symbol_index: u32,
        /// Original move site.
        move_span: Span,
        /// Use site span.
        span: Span,
    },
    /// Assignment target was moved.
    MovedAssignTarget {
        /// Span of the assignment.
        span: Span,
    },
    /// Unresolved value identifier (should not happen after resolve).
    UnresolvedValue {
        /// Name index.
        symbol_index: u32,
        /// Span.
        span: Span,
    },
    /// `break` or `continue` not inside a loop.
    LoopControlOutsideLoop {
        /// `"break"` or `"continue"`.
        keyword: &'static str,
        /// Statement span.
        span: Span,
    },
}

impl TypeCheckError {
    /// Returns the primary span for this error, if any.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        match self {
            Self::Mismatch { span, .. }
            | Self::UnknownType { span, .. }
            | Self::ArityMismatch { span, .. }
            | Self::NotCallable { span, .. }
            | Self::UnresolvedMethod { span, .. }
            | Self::AmbiguousMethod { span, .. }
            | Self::NonUnifyingBranches { span }
            | Self::InvalidCast { span, .. }
            | Self::InvalidOperator { span, .. }
            | Self::UnsupportedFeature { span, .. }
            | Self::UseAfterMove { span, .. }
            | Self::MovedAssignTarget { span }
            | Self::UnresolvedValue { span, .. }
            | Self::LoopControlOutsideLoop { span, .. } => Some(*span),
        }
    }
}

impl fmt::Display for TypeCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mismatch {
                expected, found, ..
            } => write!(f, "type mismatch: expected {expected}, found {found}"),
            Self::UnknownType { symbol_index, .. } => {
                write!(f, "unknown type (sym#{symbol_index})")
            }
            Self::ArityMismatch {
                expected, found, ..
            } => write!(
                f,
                "argument count mismatch: expected {expected}, found {found}"
            ),
            Self::NotCallable { found, .. } => write!(f, "value of type `{found}` is not callable"),
            Self::UnresolvedMethod {
                receiver,
                method_index,
                ..
            } => write!(f, "no method sym#{method_index} on type `{receiver}`"),
            Self::AmbiguousMethod {
                receiver,
                method_index,
                ..
            } => write!(
                f,
                "ambiguous method sym#{method_index} on type `{receiver}` (multiple trait impls)"
            ),
            Self::NonUnifyingBranches { .. } => f.write_str("branch types do not unify"),
            Self::InvalidCast { from, to, .. } => {
                write!(f, "invalid cast from `{from}` to `{to}`")
            }
            Self::InvalidOperator { op, .. } => write!(f, "invalid use of operator `{op}`"),
            Self::UnsupportedFeature { feature, .. } => {
                write!(f, "{feature} is not available in MVP")
            }
            Self::UseAfterMove { symbol_index, .. } => {
                write!(f, "use of moved value (sym#{symbol_index})")
            }
            Self::MovedAssignTarget { .. } => f.write_str("cannot assign to a moved value"),
            Self::UnresolvedValue { symbol_index, .. } => {
                write!(f, "unresolved value (sym#{symbol_index})")
            }
            Self::LoopControlOutsideLoop { keyword, .. } => {
                write!(f, "`{keyword}` outside of a loop")
            }
        }
    }
}

impl std::error::Error for TypeCheckError {}

/// Result of a type-check pass that may collect multiple errors.
pub type TypeCheckResult<T> = Result<T, TypeCheckBag>;

/// Collected type-check diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypeCheckBag {
    errors: Vec<TypeCheckError>,
}

impl TypeCheckBag {
    /// Creates an empty bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an error.
    pub fn push(&mut self, error: TypeCheckError) {
        self.errors.push(error);
    }

    /// Returns `true` if any errors were recorded.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Borrows collected errors.
    #[must_use]
    pub fn errors(&self) -> &[TypeCheckError] {
        &self.errors
    }

    /// Consumes the bag and returns errors.
    #[must_use]
    pub fn into_errors(self) -> Vec<TypeCheckError> {
        self.errors
    }
}

impl fmt::Display for TypeCheckBag {
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

impl std::error::Error for TypeCheckBag {}

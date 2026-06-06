//! Type-checking failure types.
//!
//! Collected in [`TypeCheckBag`] during [`phx_compiler::type_check`].

use core::fmt;

use crate::LocatedError;
use crate::Span;
use crate::code::DiagnosticCode;

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
    /// `match` on an enum does not cover all variants (and has no `_` arm).
    NonExhaustiveMatch {
        /// Uncovered variant names, in declaration order.
        missing: Vec<String>,
        /// `match` expression span.
        span: Span,
    },
    /// `match` arm can never be reached because an earlier arm covers the same cases.
    UnreachableMatchArm {
        /// Short explanation for the diagnostic.
        reason: &'static str,
        /// Unreachable arm pattern span.
        span: Span,
    },
    /// Struct literal names a field that does not exist.
    UnknownStructField {
        /// Field name.
        name: String,
        /// Field initializer span.
        span: Span,
    },
    /// Struct literal omits a required field.
    MissingStructField {
        /// Field name.
        name: String,
        /// Struct literal span.
        span: Span,
    },
    /// Enum struct-variant literal names a field that does not exist.
    UnknownEnumVariantField {
        /// Field name.
        name: String,
        /// Field initializer span.
        span: Span,
    },
    /// Enum struct-variant literal omits a required field.
    MissingEnumVariantField {
        /// Field name.
        name: String,
        /// Variant literal span.
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
        /// Variable name for diagnostics.
        name: String,
        /// Original move site.
        move_span: Span,
        /// Use site span.
        span: Span,
    },
    /// Assignment target was moved.
    MovedAssignTarget {
        /// Variable name for diagnostics.
        name: String,
        /// Original move site.
        move_span: Span,
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
    /// Type alias expands in a cycle (`type A = B; type B = A;`).
    RecursiveTypeAlias {
        /// Alias declaration span.
        span: Span,
    },
    /// Returning a slice or reference that borrows a local binding.
    ReturnEscapesLocal {
        /// `return` or trailing value span.
        span: Span,
        /// Site where the borrow of the local was formed.
        borrow_span: Span,
    },
    /// Concrete type at a generic instantiation does not implement a required trait bound.
    TraitNotSatisfied {
        /// Type that failed the bound.
        type_name: String,
        /// Required trait name.
        trait_name: String,
        /// Instantiation site span.
        span: Span,
    },
    /// Could not infer generic type arguments from call-site arguments.
    InferenceFailed {
        /// Call site span.
        span: Span,
    },
    /// Generic type argument inference produced conflicting constraints.
    InferenceAmbiguous {
        /// Call site span.
        span: Span,
    },
    /// Trait impl block does not implement a required trait method.
    MissingTraitMethod {
        /// Implementing type name.
        type_name: String,
        /// Trait name.
        trait_name: String,
        /// Required method name.
        method_name: String,
        /// Impl block span.
        span: Span,
    },
}

impl TypeCheckError {
    /// Stable diagnostic code for this error.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::Mismatch { .. } => DiagnosticCode::new("E2001"),
            Self::UnknownType { .. } => DiagnosticCode::new("E2002"),
            Self::ArityMismatch { .. } => DiagnosticCode::new("E2003"),
            Self::NotCallable { .. } => DiagnosticCode::new("E2004"),
            Self::UnresolvedMethod { .. } => DiagnosticCode::new("E2005"),
            Self::AmbiguousMethod { .. } => DiagnosticCode::new("E2006"),
            Self::NonUnifyingBranches { .. } => DiagnosticCode::new("E2007"),
            Self::NonExhaustiveMatch { .. } => DiagnosticCode::new("E2008"),
            Self::UnreachableMatchArm { .. } => DiagnosticCode::new("E2009"),
            Self::UnknownStructField { .. } => DiagnosticCode::new("E2010"),
            Self::MissingStructField { .. } => DiagnosticCode::new("E2011"),
            Self::UnknownEnumVariantField { .. } => DiagnosticCode::new("E2012"),
            Self::MissingEnumVariantField { .. } => DiagnosticCode::new("E2013"),
            Self::InvalidCast { .. } => DiagnosticCode::new("E2014"),
            Self::InvalidOperator { .. } => DiagnosticCode::new("E2015"),
            Self::UnsupportedFeature { .. } => DiagnosticCode::new("E2016"),
            Self::UseAfterMove { .. } => DiagnosticCode::new("E2017"),
            Self::MovedAssignTarget { .. } => DiagnosticCode::new("E2018"),
            Self::UnresolvedValue { .. } => DiagnosticCode::new("E2019"),
            Self::LoopControlOutsideLoop { .. } => DiagnosticCode::new("E2020"),
            Self::RecursiveTypeAlias { .. } => DiagnosticCode::new("E2021"),
            Self::ReturnEscapesLocal { .. } => DiagnosticCode::new("E2022"),
            Self::TraitNotSatisfied { .. } => DiagnosticCode::new("E2023"),
            Self::InferenceFailed { .. } => DiagnosticCode::new("E2024"),
            Self::InferenceAmbiguous { .. } => DiagnosticCode::new("E2025"),
            Self::MissingTraitMethod { .. } => DiagnosticCode::new("E2026"),
        }
    }

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
            | Self::NonExhaustiveMatch { span, .. }
            | Self::UnreachableMatchArm { span, .. }
            | Self::UnknownStructField { span, .. }
            | Self::MissingStructField { span, .. }
            | Self::UnknownEnumVariantField { span, .. }
            | Self::MissingEnumVariantField { span, .. }
            | Self::InvalidCast { span, .. }
            | Self::InvalidOperator { span, .. }
            | Self::UnsupportedFeature { span, .. }
            | Self::UseAfterMove { span, .. }
            | Self::MovedAssignTarget { span, .. }
            | Self::UnresolvedValue { span, .. }
            | Self::LoopControlOutsideLoop { span, .. }
            | Self::RecursiveTypeAlias { span, .. }
            | Self::ReturnEscapesLocal { span, .. }
            | Self::TraitNotSatisfied { span, .. }
            | Self::InferenceFailed { span, .. }
            | Self::InferenceAmbiguous { span, .. }
            | Self::MissingTraitMethod { span, .. } => Some(*span),
        }
    }
}

impl fmt::Display for TypeCheckError {
    #[allow(clippy::too_many_lines)]
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
            Self::NonExhaustiveMatch { missing, .. } => {
                if missing.is_empty() {
                    write!(f, "non-exhaustive `match` on enum")
                } else {
                    write!(
                        f,
                        "non-exhaustive `match`: missing variant(s) {}",
                        missing.join(", ")
                    )
                }
            }
            Self::UnreachableMatchArm { reason, .. } => {
                write!(f, "unreachable `match` arm: {reason}")
            }
            Self::UnknownStructField { name, .. } => {
                write!(f, "struct literal has no field `{name}`")
            }
            Self::MissingStructField { name, .. } => {
                write!(f, "struct literal is missing field `{name}`")
            }
            Self::UnknownEnumVariantField { name, .. } => {
                write!(f, "enum variant literal has no field `{name}`")
            }
            Self::MissingEnumVariantField { name, .. } => {
                write!(f, "enum variant literal is missing field `{name}`")
            }
            Self::InvalidCast { from, to, .. } => {
                write!(f, "invalid cast from `{from}` to `{to}`")
            }
            Self::InvalidOperator { op, .. } => write!(f, "invalid use of operator `{op}`"),
            Self::UnsupportedFeature { feature, .. } => {
                write!(f, "{feature} is not available in MVP")
            }
            Self::UseAfterMove { name, .. } => {
                write!(f, "use of moved value `{name}`")
            }
            Self::MovedAssignTarget { name, .. } => {
                write!(f, "cannot assign to moved value `{name}`")
            }
            Self::UnresolvedValue { symbol_index, .. } => {
                write!(f, "unresolved value (sym#{symbol_index})")
            }
            Self::LoopControlOutsideLoop { keyword, .. } => {
                write!(f, "`{keyword}` outside of a loop")
            }
            Self::RecursiveTypeAlias { .. } => f.write_str("recursive type alias"),
            Self::ReturnEscapesLocal { .. } => {
                f.write_str("cannot return a borrow of a local variable")
            }
            Self::TraitNotSatisfied {
                type_name,
                trait_name,
                ..
            } => write!(
                f,
                "type `{type_name}` does not satisfy trait bound `{trait_name}`"
            ),
            Self::InferenceFailed { .. } => {
                f.write_str("could not infer generic type arguments from call arguments")
            }
            Self::InferenceAmbiguous { .. } => {
                f.write_str("ambiguous generic type argument inference")
            }
            Self::MissingTraitMethod {
                type_name,
                trait_name,
                method_name,
                ..
            } => write!(
                f,
                "type `{type_name}` does not implement trait method `{method_name}` from `{trait_name}`"
            ),
        }
    }
}

impl std::error::Error for TypeCheckError {}

/// Result of a type-check pass that may collect multiple errors.
pub type TypeCheckResult<T> = Result<T, TypeCheckBag>;

/// Collected type-check diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypeCheckBag {
    errors: Vec<LocatedError<TypeCheckError>>,
}

impl TypeCheckBag {
    /// Creates an empty bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an error for `module`.
    pub fn push(&mut self, module: u32, error: TypeCheckError) {
        self.errors.push(LocatedError::new(module, error));
    }

    /// Records an already-located error.
    pub fn push_located(&mut self, located: LocatedError<TypeCheckError>) {
        self.errors.push(located);
    }

    /// Returns `true` if any errors were recorded.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Borrows collected located errors.
    #[must_use]
    pub fn errors(&self) -> &[LocatedError<TypeCheckError>] {
        &self.errors
    }

    /// Consumes the bag and returns located errors.
    #[must_use]
    pub fn into_errors(self) -> Vec<LocatedError<TypeCheckError>> {
        self.errors
    }
}

impl fmt::Display for TypeCheckBag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, located) in self.errors.iter().enumerate() {
            if i > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{}", located.error)?;
        }
        Ok(())
    }
}

impl std::error::Error for TypeCheckBag {}

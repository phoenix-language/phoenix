//! Type-checking failure types.
//!
//! Collected in [`TypeCheckBag`] during [`phx_compiler::type_check`].

use core::fmt;

use crate::LocatedError;
use crate::Span;

/// Why a [`TypeCheckError::Mismatch`] was reported (drives secondary notes and help text).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum MismatchKind {
    /// Generic expression context (call argument, operator, pattern, etc.).
    #[default]
    Expression,
    /// `const name: T = expr` where `T` does not match `expr`.
    ConstBinding {
        /// Binding identifier.
        name: String,
        /// Span of the type annotation (`T`).
        annotation_span: Span,
    },
    /// `var name: T = expr` where `T` does not match `expr`.
    VarBinding {
        /// Binding identifier.
        name: String,
        /// Span of the type annotation (`T`).
        annotation_span: Span,
    },
    /// `return expr` where `expr` does not match the function return type.
    Return,
    /// Function body value does not match the declared return type.
    FunctionBody,
    /// Call argument at `index` (0-based) does not match the parameter type.
    Argument {
        /// Argument index.
        index: usize,
    },
    /// Assignment target type does not match the assigned value.
    Assign {
        /// Target identifier when the assignee is a simple local.
        name: Option<String>,
    },
    /// Struct literal field initializer type does not match the field type.
    StructField {
        /// Field name.
        name: String,
    },
    /// Enum struct-variant literal field initializer type does not match.
    EnumVariantField {
        /// Field name.
        name: String,
    },
    /// Boolean `if` condition is not `bool`.
    Condition,
}

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
        /// Primary span (usually the expression with the wrong type).
        span: Span,
        /// Where the expectation came from (binding annotation, return type, etc.).
        kind: MismatchKind,
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
    /// Trait name in a generic bound does not resolve to a known trait definition.
    UnknownTraitBound {
        /// Unresolved trait name.
        trait_name: String,
        /// Bound site span.
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
    /// Trait impl block does not specify a required associated type.
    MissingAssociatedType {
        /// Implementing type name.
        type_name: String,
        /// Trait name.
        trait_name: String,
        /// Required associated type name.
        assoc_name: String,
        /// Impl block span.
        span: Span,
    },
    /// `?` used outside a function with a compatible return type.
    TryOutsideFunction {
        /// Use site span.
        span: Span,
    },
    /// `?` operand is not a std `Option` / `Result` compatible with the enclosing return type.
    InvalidTryOperand {
        /// Operand type description.
        found: String,
        /// Enclosing function return type description.
        expected_return: String,
        /// Use site span.
        span: Span,
    },
    /// `?` on `Result` with mismatched error types and no `From` impl.
    TryErrorFromMissing {
        /// Scrutinee `Err` payload type.
        err_in: String,
        /// Enclosing function `Err` type.
        err_out: String,
        /// Use site span.
        span: Span,
    },
    /// `extern "C"` call requires an `unsafe` block or `unsafe fn`.
    ExternCallRequiresUnsafe {
        /// Foreign symbol name.
        name: String,
        /// Call site span.
        span: Span,
    },
    /// VM intrinsic call requires an `unsafe` block or `unsafe fn`.
    IntrinsicRequiresUnsafe {
        /// Intrinsic name.
        name: String,
        /// Call site span.
        span: Span,
    },
    /// Call to an effectively-unsafe function requires an `unsafe` block or `unsafe fn`.
    UnsafeFnCallRequiresUnsafe {
        /// Callee name.
        name: String,
        /// Call site span.
        span: Span,
    },
    /// An `unsafe trait` requires an `unsafe impl`.
    UnsafeTraitRequiresUnsafeImpl {
        /// Trait name.
        trait_name: String,
        /// Impl block span.
        span: Span,
    },
    /// `unsafe` on a trait method is redundant inside an `unsafe trait`.
    RedundantUnsafeInUnsafeTrait {
        /// Method name.
        method: String,
        /// Method span.
        span: Span,
    },
    /// `unsafe impl` is only valid for an `unsafe trait`.
    UnsafeImplOfSafeTrait {
        /// Implementing type name.
        type_name: String,
        /// Impl block span.
        span: Span,
    },
    /// Type implements both `Drop` and `Copyable`, which conflict.
    CopyableDropConflict {
        /// Type name for diagnostics.
        type_name: String,
        /// Impl block span.
        span: Span,
    },
    /// Internal compiler invariant violation during type checking.
    InternalError {
        /// Short invariant description.
        detail: &'static str,
        /// Related source span.
        span: Span,
    },
}

impl fmt::Display for TypeCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct RawSymbolNames;

        impl crate::SymbolNames for RawSymbolNames {
            fn symbol_name(&self, _symbol_index: u32) -> Option<&str> {
                None
            }
        }

        f.write_str(&crate::format::typecheck_message(&RawSymbolNames, self))
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

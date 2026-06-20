//! Type-checking failure types for the Phoenix compiler.
//!
//! Produced by [`phx_compiler::unstable::type_check`] while walking the resolved AST, checking
//! expression types, ownership, trait impls, and MVP language rules. The type checker collects
//! non-fatal errors in [`TypeCheckBag`] (wrapped in [`LocatedError`] for multi-module crates)
//! and may continue after each failure so callers see every issue in one pass.
//!
//! ## Compiler pass
//!
//! Type checking follows name resolution ([`ResolveError`]) and precedes lowering
//! ([`LowerError`]). On success, the driver merges trait defaults and runs monomorphization;
//! on failure, [`TypeCheckBag`] is returned without a [`TypedProgram`].
//!
//! ## Diagnostic codes (E2001–E2048)
//!
//! Stable codes are assigned in [`crate::type_error_registry`] via the
//! [`typecheck_error_registry!`] macro. Each variant maps to exactly one code; [`TypeCheckError::code`]
//! and [`TypeCheckError::span`] are generated from that table.
//!
//! | Code | Variant | Summary |
//! |------|---------|---------|
//! | E2001 | [`TypeCheckError::Mismatch`] | Expression type does not match expectation |
//! | E2002 | [`TypeCheckError::UnknownType`] | Type name not in scope |
//! | E2003 | [`TypeCheckError::ArityMismatch`] | Call argument count does not match parameters |
//! | E2004 | [`TypeCheckError::NotCallable`] | Callee expression is not a function type |
//! | E2005 | [`TypeCheckError::UnresolvedMethod`] | No method with this name on the receiver |
//! | E2006 | [`TypeCheckError::AmbiguousMethod`] | Multiple trait impls provide the same method |
//! | E2007 | [`TypeCheckError::NonUnifyingBranches`] | `if` or `match` arms do not unify |
//! | E2008 | [`TypeCheckError::NonExhaustiveMatch`] | `match` missing variant arms (no `_`) |
//! | E2009 | [`TypeCheckError::UnreachableMatchArm`] | Match arm covered by an earlier arm |
//! | E2010 | [`TypeCheckError::UnknownStructField`] | Struct literal names a nonexistent field |
//! | E2011 | [`TypeCheckError::MissingStructField`] | Struct literal omits a required field |
//! | E2012 | [`TypeCheckError::UnknownEnumVariantField`] | Enum struct-variant field unknown |
//! | E2013 | [`TypeCheckError::MissingEnumVariantField`] | Enum struct-variant field missing |
//! | E2014 | [`TypeCheckError::InvalidCast`] | Explicit cast not allowed between types |
//! | E2015 | [`TypeCheckError::InvalidOperator`] | Operator cannot apply to operand types |
//! | E2016 | [`TypeCheckError::UnsupportedFeature`] | Post-MVP language or std feature |
//! | E2017 | [`TypeCheckError::UseAfterMove`] | Use of a moved binding |
//! | E2018 | [`TypeCheckError::MovedAssignTarget`] | Assignment to a moved binding |
//! | E2019 | [`TypeCheckError::UnresolvedValue`] | Value identifier unresolved after resolve |
//! | E2020 | [`TypeCheckError::LoopControlOutsideLoop`] | `break` / `continue` outside a loop |
//! | E2021 | [`TypeCheckError::RecursiveTypeAlias`] | Cyclic `type` alias expansion |
//! | E2022 | [`TypeCheckError::ReturnEscapesLocal`] | Return borrows a local binding |
//! | E2023 | [`TypeCheckError::TraitNotSatisfied`] | Concrete type missing a trait bound |
//! | E2024 | [`TypeCheckError::InferenceFailed`] | Generic type args could not be inferred |
//! | E2025 | [`TypeCheckError::InferenceAmbiguous`] | Conflicting generic inference constraints |
//! | E2026 | [`TypeCheckError::MissingTraitMethod`] | Trait impl missing a required method |
//! | E2027 | [`TypeCheckError::MissingAssociatedType`] | Trait impl missing an associated type |
//! | E2028 | [`TypeCheckError::TryOutsideFunction`] | `?` used outside a function body |
//! | E2029 | [`TypeCheckError::InvalidTryOperand`] | `?` operand not compatible with return type |
//! | E2030 | [`TypeCheckError::UnknownTraitBound`] | Trait name in a bound does not resolve |
//! | E2031 | [`TypeCheckError::TryErrorFromMissing`] | `Result` error types lack a `From` impl |
//! | E2032 | [`TypeCheckError::ExternCallRequiresUnsafe`] | `extern "C"` call needs `unsafe` |
//! | E2033 | [`TypeCheckError::CopyableDropConflict`] | Type implements both `Drop` and `Copyable` |
//! | E2034 | [`TypeCheckError::IntrinsicRequiresUnsafe`] | VM intrinsic call needs `unsafe` |
//! | E2035 | [`TypeCheckError::UnsafeFnCallRequiresUnsafe`] | Unsafe callee needs `unsafe` context |
//! | E2036 | [`TypeCheckError::UnsafeTraitRequiresUnsafeImpl`] | `unsafe trait` needs `unsafe impl` |
//! | E2037 | [`TypeCheckError::RedundantUnsafeInUnsafeTrait`] | Redundant `unsafe` on trait method |
//! | E2038 | [`TypeCheckError::UnsafeImplOfSafeTrait`] | `unsafe impl` on a safe trait |
//! | E2039 | [`TypeCheckError::InternalError`] | Internal compiler invariant violation |
//! | E2040 | [`TypeCheckError::ProgramTooLarge`] | Definition table exceeded `u32::MAX` |
//! | E2041 | [`TypeCheckError::DiscardedStdResult`] | Discarded `std::Result` statement value |
//! | E2042 | [`TypeCheckError::DiscardedStdOption`] | Discarded `std::Option` statement value |
//! | E2043 | [`TypeCheckError::LangItemReserved`] | `#[lang_item]` outside the standard library |
//! | E2044 | [`TypeCheckError::LangItemDuplicate`] | Duplicate language item registration |
//! | E2045 | [`TypeCheckError::LangItemInvalid`] | Malformed `#[lang_item]` attribute |
//! | E2046 | [`TypeCheckError::GenericNestingTooDeep`] | Generic nesting exceeds monomorph limit |
//! | E2047 | [`TypeCheckError::OverlappingMutBorrow`] | Two active `&mut` borrows of one binding |
//! | E2048 | [`TypeCheckError::SharedMutBorrowConflict`] | Shared and mutable borrow overlap |
//!
//! ## Integration with [`crate::format`] and ancillary notes
//!
//! - **Registry** — [`crate::type_error_registry`] owns the variant → code mapping and generates
//!   [`TypeCheckError::code`] / [`TypeCheckError::span`]. When adding a variant, update the
//!   registry table, explain text in [`crate::render::explain_code`], and (if needed) a match arm
//!   in [`crate::type_notes::typecheck_ancillary`].
//! - **Message text** — [`typecheck_message`] resolves interned `symbol_index` / `method_index`
//!   fields through [`SymbolNames`] and mirrors the prose used by [`TypeCheckError`]'s
//!   [`Display`] impl.
//! - **Secondary notes** — [`crate::type_notes::typecheck_ancillary`] supplies `= note:` and
//!   `= help:` lines (move sites, binding annotations, arity hints, trait suggestions). Used by
//!   [`format_typecheck_error_styled`]; [`MismatchKind`] drives mismatch-specific notes.
//! - **Single error** — [`format_typecheck_error`] / [`format_typecheck_error_styled`] render
//!   Cargo-style headers plus carets when source and span are available.
//! - **Multiple errors** — [`TypeCheckBag`] errors are formatted individually by callers (the
//!   CLI iterates [`TypeCheckBag::errors`] and applies [`format_typecheck_error_styled`] per
//!   [`LocatedError`]).
//!
//! [`LowerError`]: crate::LowerError
//! [`ResolveError`]: crate::ResolveError
//! [`SymbolNames`]: crate::SymbolNames
//! [`TypedProgram`]: phx_compiler::typeck::TypedProgram
//! [`typecheck_error_registry!`]: crate::type_error_registry
//! [`typecheck_message`]: crate::format::typecheck_message
//! [`format_typecheck_error`]: crate::format::format_typecheck_error
//! [`format_typecheck_error_styled`]: crate::format::format_typecheck_error_styled

use core::fmt;

use crate::LocatedError;
use crate::Span;

/// Why a [`TypeCheckError::Mismatch`] was reported.
///
/// Drives secondary notes and help text in [`crate::type_notes::typecheck_ancillary`]. For
/// binding kinds, the annotation span is attached as a `= note:` pointing at the declared type.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum MismatchKind {
    /// Generic expression context (call argument, operator, pattern, etc.).
    #[default]
    Expression,
    /// `const name: T = expr` where `T` does not match `expr`.
    ///
    /// Notes reference the binding name and the type annotation span.
    ConstBinding {
        /// Binding identifier.
        name: String,
        /// Span of the type annotation (`T`).
        annotation_span: Span,
    },
    /// `var name: T = expr` where `T` does not match `expr`.
    ///
    /// Notes reference the binding name and the type annotation span.
    VarBinding {
        /// Binding identifier.
        name: String,
        /// Span of the type annotation (`T`).
        annotation_span: Span,
    },
    /// `return expr` where `expr` does not match the function return type.
    Return,
    /// Function body value does not match the declared return type.
    ///
    /// Emitted for implicit trailing expressions, not only explicit `return`.
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
///
/// Each variant maps to a stable [`DiagnosticCode`](crate::code::DiagnosticCode) via
/// [`TypeCheckError::code`] (E2001–E2048). Primary source locations are available through
/// [`TypeCheckError::span`] for caret rendering in [`crate::format::format_typecheck_error`].
/// Variants with secondary spans (move sites, borrow sites, duplicate lang items) attach extra
/// notes through [`crate::type_notes::typecheck_ancillary`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TypeCheckError {
    /// Expression or binding type does not match expectation.
    ///
    /// Core type-compatibility failure. The `expected` and `found` strings are short type
    /// descriptions (not necessarily source syntax). [`MismatchKind`] records where the expectation
    /// originated so formatters can add binding or return-type notes.
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
    ///
    /// Distinct from [`ResolveError::UnresolvedType`](crate::ResolveError::UnresolvedType): emitted
    /// when resolve succeeded but the type checker cannot map a name to a known definition
    /// (for example after generic substitution).
    UnknownType {
        /// Interned name index (display via [`SymbolNames`](crate::SymbolNames)).
        symbol_index: u32,
        /// Use site span.
        span: Span,
    },
    /// Call argument count does not match parameters.
    ///
    /// Compares positional argument count only; variadic or default parameters are not in MVP.
    /// Help text suggests adding or removing arguments.
    ArityMismatch {
        /// Expected parameter count.
        expected: usize,
        /// Found argument count.
        found: usize,
        /// Call site span.
        span: Span,
    },
    /// Expression is not callable.
    ///
    /// The callee's type is not a function, method table, or other callable form. Help text
    /// suggests checking the callee or using method-call syntax.
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
        /// Method name index (display via [`SymbolNames`](crate::SymbolNames)).
        method_index: u32,
        /// Call site span.
        span: Span,
    },
    /// Multiple trait impls provide the same method name.
    ///
    /// Emitted when disambiguation by receiver type alone is insufficient. Help text lists
    /// candidate traits when available.
    AmbiguousMethod {
        /// Receiver type description.
        receiver: String,
        /// Method name index.
        method_index: u32,
        /// Call site span.
        span: Span,
    },
    /// `if` or `match` arms do not unify to one type.
    ///
    /// Phoenix requires all branches of a conditional expression to share one type. The span
    /// typically covers the whole `if` or `match` expression.
    NonUnifyingBranches {
        /// Branch span.
        span: Span,
    },
    /// `match` on an enum does not cover all variants (and has no `_` arm).
    ///
    /// `missing` lists uncovered variant names in declaration order. Help text suggests adding
    /// arms or a wildcard pattern.
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
    ///
    /// MVP allows only a fixed set of cast targets (numeric primitives, array→slice, str→`[u8]`).
    /// Help text summarizes supported casts.
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
    ///
    /// The `feature` string is a stable identifier for tests, `phx explain`, and diagnostic
    /// goldens (not user-facing prose).
    UnsupportedFeature {
        /// Short feature name for the diagnostic.
        feature: &'static str,
        /// Use site span.
        span: Span,
    },
    /// Use of a binding after it was moved.
    ///
    /// Phoenix ownership guarantee: a moved value cannot be used again. Formatters attach a
    /// secondary note at `move_span` via [`crate::type_notes::typecheck_ancillary`].
    UseAfterMove {
        /// Variable name for diagnostics.
        name: String,
        /// Original move site.
        move_span: Span,
        /// Use site span.
        span: Span,
    },
    /// Assignment target was moved.
    ///
    /// Like [`TypeCheckError::UseAfterMove`], but the illegal use is the assignment target.
    /// Secondary note points at the original move site.
    MovedAssignTarget {
        /// Variable name for diagnostics.
        name: String,
        /// Original move site.
        move_span: Span,
        /// Span of the assignment.
        span: Span,
    },
    /// Unresolved value identifier (should not happen after resolve).
    ///
    /// Indicates an internal pipeline bug or stale AST if resolve reported success. Still surfaced
    /// as a user-facing diagnostic rather than a compiler panic.
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
    ///
    /// Lifetime escape check for MVP return types. Secondary note at `borrow_span` shows where
    /// the borrow of the local was formed.
    ReturnEscapesLocal {
        /// `return` or trailing value span.
        span: Span,
        /// Site where the borrow of the local was formed.
        borrow_span: Span,
    },
    /// Two overlapping `&mut` borrows of the same local binding.
    OverlappingMutBorrow {
        /// Borrowed binding name.
        name: String,
        /// First mutable borrow site.
        prior_span: Span,
        /// Second (conflicting) borrow site.
        span: Span,
    },
    /// Shared and mutable borrows of the same local binding overlap.
    SharedMutBorrowConflict {
        /// Borrowed binding name.
        name: String,
        /// Prior conflicting borrow site.
        prior_span: Span,
        /// New (conflicting) borrow site.
        span: Span,
        /// Whether the new borrow is `&mut` (true) or shared `&` (false).
        new_borrow_is_mut: bool,
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
    ///
    /// Requires a `From<Err_in>` impl (or identical error types) for the enclosing function's
    /// `Result` error parameter.
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
    ///
    /// Phoenix forbids types that are both droppable and bitwise-copyable.
    CopyableDropConflict {
        /// Type name for diagnostics.
        type_name: String,
        /// Impl block span.
        span: Span,
    },
    /// Internal compiler invariant violation during type checking.
    ///
    /// Indicates a bug in the compiler, not invalid user source. The `detail` string is for
    /// maintainers; users see a generic internal-error message.
    InternalError {
        /// Short invariant description.
        detail: &'static str,
        /// Related source span.
        span: Span,
    },
    /// Definition table exceeded `u32::MAX` entries.
    ///
    /// Resource limit rather than a type mistake; still reported at a related source span.
    ProgramTooLarge {
        /// Related source span.
        span: Span,
    },
    /// A std `Result` value was used as a discarded statement expression.
    ///
    /// Phoenix requires explicit handling of `Result` (match, `?`, or binding). Help text
    /// suggests using `?` or matching on the value.
    DiscardedStdResult {
        /// Discarded expression span.
        span: Span,
    },
    /// A std `Option` value was used as a discarded statement expression.
    ///
    /// Like [`TypeCheckError::DiscardedStdResult`], but for `Option`. Help text suggests
    /// matching or propagating with `?`.
    DiscardedStdOption {
        /// Discarded expression span.
        span: Span,
    },
    /// `#[lang_item]` used outside the standard library.
    LangItemReserved {
        /// Attribute span.
        span: Span,
    },
    /// Duplicate `(kind, name)` language item registration.
    ///
    /// Secondary note at `previous_span` identifies the first registration.
    LangItemDuplicate {
        /// Item kind string.
        kind: String,
        /// Item name.
        name: String,
        /// Duplicate declaration span.
        span: Span,
        /// First declaration span.
        previous_span: Span,
    },
    /// Invalid `#[lang_item]` attribute.
    LangItemInvalid {
        /// Detail message.
        detail: String,
        /// Attribute span.
        span: Span,
    },
    /// Generic type nesting exceeds the monomorphization depth limit.
    GenericNestingTooDeep {
        /// Measured nesting depth.
        depth: usize,
        /// Configured maximum depth.
        limit: usize,
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
///
/// - **`Ok(T)`** — Type checking succeeded; `T` is typically [`TypedProgram`](phx_compiler::typeck::TypedProgram).
/// - **`Err` carrying a [`TypeCheckBag`]** — One or more type errors were collected. The bag may contain
///   multiple [`LocatedError`] entries when checking continues after non-fatal failures.
///
/// # Errors
///
/// Returns [`TypeCheckBag`] when the type checker recorded any diagnostic. The bag is never
/// empty in the `Err` case.
pub type TypeCheckResult<T> = Result<T, TypeCheckBag>;

/// Collected type-check diagnostics; checking may continue after non-fatal errors.
///
/// Unlike a fatal `Result` return from an inner helper, a [`TypeCheckBag`] lets the type checker
/// finish the crate and report every issue in one pass. Each entry is a [`LocatedError`] so
/// multi-module builds attribute failures to a module id before the CLI maps spans to file paths.
///
/// Format individual entries with [`crate::format::format_typecheck_error_styled`] (the CLI
/// iterates [`TypeCheckBag::errors`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypeCheckBag {
    errors: Vec<LocatedError<TypeCheckError>>,
}

impl TypeCheckBag {
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
    /// Wraps `error` in [`LocatedError::new`] with the given module id.
    ///
    /// # Panics
    ///
    /// Never panics.
    pub fn push(&mut self, module: u32, error: TypeCheckError) {
        self.errors.push(LocatedError::new(module, error));
    }

    /// Records an already-located error.
    ///
    /// # Panics
    ///
    /// Never panics.
    pub fn push_located(&mut self, located: LocatedError<TypeCheckError>) {
        self.errors.push(located);
    }

    /// Returns `true` if any errors were recorded.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Borrows collected located errors in insertion order.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn errors(&self) -> &[LocatedError<TypeCheckError>] {
        &self.errors
    }

    /// Consumes the bag and returns located errors in insertion order.
    ///
    /// # Panics
    ///
    /// Never panics.
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

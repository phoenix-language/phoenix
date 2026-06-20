//! Definition identifiers and kinds produced by name resolution.
//!
//! ## Pass role
//!
//! Defines the dense definition table vocabulary used by [`super::Resolver`] during the AST walk in
//! [`super::walk`]. Every binding introduced while resolving receives a [`DefId`] index into
//! [`super::ResolvedProgram::defs`]; [`DefKind`] classifies its syntactic role and [`Def`] stores
//! the interned name, defining span, owning module, export flag, and lexical scope depth.
//!
//! ## Inputs and outputs
//!
//! | Type | Written by | Read by |
//! | --- | --- | --- |
//! | [`DefId`] | [`super::Resolver::alloc_def`] during definition collection and the AST walk | [`super::ResolvedProgram::resolutions`], [`super::ResolvedProgram::closures`], typeck, lowering |
//! | [`DefKind`] | Same allocation sites, one per [`Def`] | Typeck (namespace checks, mono kinds), display helpers |
//! | [`Def`] | Same allocation sites | Scope duplicate diagnostics, closure upvar depth checks, typeck name display |
//!
//! [`DefId`] values are **not** a stable ABI across compiler versions — treat them as in-memory
//! handles for one [`super::ResolvedProgram`] instance.

use phx_diagnostics::Span;
use phx_syntax::Symbol;

/// Dense index into [`super::ResolvedProgram::defs`].
///
/// Ids are allocated sequentially during resolve (starting at `0`) and index both locally defined
/// bindings and imported symbols wired in from dependency `.pxi` files in multi-module builds. A
/// [`DefId`] is the canonical handle for a named definition for the rest of the compiler pipeline:
///
/// - [`super::ResolvedProgram::resolutions`] maps each name-use site to a defining [`DefId`].
/// - [`super::ResolvedProgram::closures`] keys closure capture metadata by closure [`DefId`].
/// - [`super::ResolvedProgram::main_fn`] and export tables store optional [`DefId`] values.
///
/// Lookup the full record with [`DefId::index`] as a `usize` index into [`super::ResolvedProgram::defs`].
///
/// # Stability
///
/// Not a stable ABI across compiler versions or between separate resolve runs. In-tree tests may
/// construct ids with [`DefId::from_raw`]; production code should use ids from resolve output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefId(u32);

/// Error when a dense def index does not fit in `u32`.
///
/// Returned by [`DefId::try_from_index`] when the definition table would exceed `u32::MAX` entries.
/// The resolver treats this as a hard allocation failure — it is not surfaced as a user diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DefIdOverflow;

impl DefId {
    /// Constructs a [`DefId`] from a raw table index.
    ///
    /// Intended for tests and in-tree tooling that already hold a def-table index. Production code
    /// should use ids allocated by [`super::Resolver`] rather than fabricating them.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub const fn from_raw(index: u32) -> Self {
        Self(index)
    }

    /// Maps a table length or index to a [`DefId`], or fails when it exceeds `u32::MAX`.
    ///
    /// Used when growing [`super::ResolvedProgram::defs`] to guard against `usize` indices that
    /// cannot be stored in the dense `u32` handle.
    ///
    /// # Errors
    ///
    /// Returns [`DefIdOverflow`] when `index` does not fit in `u32`.
    pub(crate) fn try_from_index(index: usize) -> Result<Self, DefIdOverflow> {
        u32::try_from(index).map(Self).map_err(|_| DefIdOverflow)
    }

    /// Returns the dense index into [`super::ResolvedProgram::defs`].
    ///
    /// Safe to use as `defs[self.index() as usize]` when the id came from the same
    /// [`super::ResolvedProgram`].
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{DefId, DefIdOverflow};

    #[test]
    fn try_from_index_accepts_u32_max() {
        assert_eq!(
            DefId::try_from_index(u32::MAX as usize),
            Ok(DefId::from_raw(u32::MAX))
        );
    }

    #[test]
    fn try_from_index_rejects_overflow() {
        assert_eq!(
            DefId::try_from_index(u32::MAX as usize + 1),
            Err(DefIdOverflow)
        );
    }
}

/// Syntactic category of a named definition in the resolve table.
///
/// Classifies each [`Def`] record so later passes can apply namespace rules without re-walking the
/// AST shape. Variants fall into three groups:
///
/// **Value namespace** — participate in expression and pattern lookup via
/// [`super::scopes::ScopeStack::lookup_value`]: [`Self::Fn`], [`Self::ExternFn`], [`Self::Const`],
/// [`Self::Var`], [`Self::Param`], [`Self::Local`], [`Self::EnumVariant`], [`Self::StructField`],
/// [`Self::Closure`].
///
/// **Type namespace** — participate in type and path lookup via
/// [`super::scopes::ScopeStack::lookup_type`]: [`Self::Struct`], [`Self::Enum`], [`Self::TypeAlias`],
/// [`Self::Trait`], [`Self::Impl`], [`Self::GenericParam`], [`Self::TraitAssocType`].
///
/// **Def-table only** — recorded in [`super::ResolvedProgram::defs`] but not registered as a
/// module-scope binding: [`Self::ImplMethod`] (body owned by parent impl), [`Self::Closure`]
/// (synthetic name; capture table in [`super::ResolvedProgram::closures`]).
///
/// New language constructs may add variants; the enum is [`non_exhaustive`] for embedders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DefKind {
    /// Top-level or nested function (`fn` item).
    Fn,
    /// Trait or inherent impl method body (`Type :: impl [ :: Trait ] { … }`).
    ///
    /// Stored as its own [`Def`] for lowering and capture analysis but not inserted into module
    /// scope maps — call sites resolve through the parent type or vtable, not by method name alone.
    ImplMethod,
    /// `extern "C"` foreign function signature (no Phoenix body).
    ExternFn,
    /// `const` binding at module or item scope.
    Const,
    /// `var` binding at module or item scope.
    Var,
    /// Function parameter or `self` in a signature list.
    Param,
    /// Pattern binding, `let`, or other block-local name.
    Local,
    /// `Name :: struct` type definition.
    Struct,
    /// `Name :: enum` type definition.
    Enum,
    /// Enum variant constructor (`Variant` or `Variant(Type, …)`).
    EnumVariant,
    /// Named field in a struct definition.
    StructField,
    /// `type Name = …` alias.
    TypeAlias,
    /// `Name :: trait` definition.
    Trait,
    /// `Type :: impl` block header (inherent or trait impl container).
    Impl,
    /// Generic type parameter (`T` in `fn f<T>(…)` or `struct S<T>`).
    GenericParam,
    /// Closure expression (synthetic def name; environment in [`super::ResolvedProgram::closures`]).
    Closure,
    /// Associated type item inside a trait definition.
    TraitAssocType,
}

/// One named binding recorded during name resolution.
///
/// Each [`Def`] is an element of [`super::ResolvedProgram::defs`], indexed by [`DefId`]. Resolve the
/// printable name via [`Self::name`] through the program [`Interner`](phx_syntax::Interner).
///
/// [`Self::scope_depth`] records the lexical nesting depth at introduction (`0` = module scope).
/// Closure capture analysis compares use-site depth against `scope_depth` to detect upvars: a
/// referenced [`DefId`] with shallower depth than the closure body was defined in an outer scope.
///
/// [`Self::module`] matches [`super::ResolutionKey::module`] and [`super::SourceModule::id`] for the
/// owning source file. Imported defs from `.pxi` files retain the importing module id where they were
/// wired into scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Def {
    /// Syntactic role of this binding ([`DefKind`]).
    pub kind: DefKind,
    /// Interned identifier for this definition.
    pub name: Symbol,
    /// Source span of the defining name token (used in duplicate-definition diagnostics).
    pub span: Span,
    /// Owning module id (matches [`super::ResolutionKey::module`]).
    pub module: u32,
    /// `true` when the top-level item is exported (`pub`) from its defining module.
    pub exported: bool,
    /// Lexical scope depth when the binding was introduced (`0` = module scope).
    pub scope_depth: u32,
}

impl DefKind {
    /// Returns `true` when this definition has an executable Phoenix body for lowering.
    ///
    /// Matches [`Self::Fn`] and [`Self::ImplMethod`] only. Extern signatures ([`Self::ExternFn`]),
    /// trait method stubs without bodies, and non-function kinds return `false`.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub const fn is_function_body(self) -> bool {
        matches!(self, DefKind::Fn | DefKind::ImplMethod)
    }
}

impl Def {
    /// Builds a definition record for tests and manual table construction.
    ///
    /// Production resolve allocates defs through [`super::Resolver`]; this constructor does not
    /// register the binding in [`super::scopes::ScopeStack`] or [`super::ResolvedProgram::resolutions`].
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub const fn new(
        kind: DefKind,
        name: Symbol,
        span: Span,
        module: u32,
        exported: bool,
        scope_depth: u32,
    ) -> Self {
        Self {
            kind,
            name,
            span,
            module,
            exported,
            scope_depth,
        }
    }
}

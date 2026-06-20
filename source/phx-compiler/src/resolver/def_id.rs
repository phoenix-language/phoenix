//! Definition identifiers and kinds produced by name resolution.
//!
//! Each binding introduced during resolve receives a dense [`DefId`] into
//! [`super::ResolvedProgram::defs`]. [`DefKind`] classifies the syntactic role; [`Def`] stores the
//! interned name, defining span, owning module, export flag, and lexical scope depth.

use phx_diagnostics::Span;
use phx_syntax::Symbol;

/// Dense index into [`super::ResolvedProgram::defs`].
///
/// Ids are allocated sequentially during resolve and index both local definitions and imported
/// symbols wired in from dependency `.pxi` files in multi-module builds. Used as keys in
/// [`super::ResolvedProgram::resolutions`] and [`super::ResolvedProgram::closures`].
///
/// Not a stable ABI across compiler versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefId(u32);

/// Error when a dense def index does not fit in `u32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DefIdOverflow;

impl DefId {
    /// Constructs a [`DefId`] from a raw table index.
    ///
    /// Intended for tests and in-tree tooling that already hold a def-table index. Production code
    /// should use ids from resolve output rather than fabricating them.
    #[must_use]
    pub const fn from_raw(index: u32) -> Self {
        Self(index)
    }

    /// Maps a table length or index to a [`DefId`], or fails when it exceeds `u32::MAX`.
    ///
    /// # Errors
    ///
    /// Returns [`DefIdOverflow`] when `index` does not fit in `u32`.
    pub(crate) fn try_from_index(index: usize) -> Result<Self, DefIdOverflow> {
        u32::try_from(index).map(Self).map_err(|_| DefIdOverflow)
    }

    /// Returns the dense index into [`super::ResolvedProgram::defs`].
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
/// Value-namespace kinds participate in expression lookup; type-namespace kinds participate in type
/// lookup. Some variants ([`Self::ImplMethod`], [`Self::Closure`]) exist only as def records and are
/// not registered as module-scope bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DefKind {
    /// Top-level or nested function.
    Fn,
    /// Trait or inherent impl method body (`Type :: impl [ :: Trait ] { … }`).
    ImplMethod,
    /// `extern "C"` foreign function signature.
    ExternFn,
    /// `const` binding.
    Const,
    /// `var` binding.
    Var,
    /// Function parameter or `self`.
    Param,
    /// Pattern or block-local binding.
    Local,
    /// `Name :: struct`.
    Struct,
    /// `Name :: enum`.
    Enum,
    /// Enum variant.
    EnumVariant,
    /// Struct field in a definition.
    StructField,
    /// `type Name = …`.
    TypeAlias,
    /// `Name :: trait`.
    Trait,
    /// `Type :: impl`.
    Impl,
    /// Generic type parameter.
    GenericParam,
    /// Closure expression (synthetic name; capture table in [`super::ResolvedProgram::closures`]).
    Closure,
    /// Trait associated type.
    TraitAssocType,
}

/// One named binding recorded during name resolution.
///
/// Resolve the printable name via [`Self::name`] through the program
/// [`Interner`](phx_syntax::Interner). [`Self::scope_depth`] is the lexical nesting depth at
/// introduction and is used to detect closure upvars (captured defs have shallower depth).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Def {
    /// Syntactic role of this binding ([`DefKind`]).
    pub kind: DefKind,
    /// Interned identifier for this definition.
    pub name: Symbol,
    /// Source span of the defining name token.
    pub span: Span,
    /// Owning module id (matches [`super::ResolutionKey::module`]).
    pub module: u32,
    /// `true` when the top-level item is exported (`pub`).
    pub exported: bool,
    /// Lexical scope depth when the binding was introduced (0 = module scope).
    pub scope_depth: u32,
}

impl DefKind {
    /// Returns `true` when this definition has an executable body for lowering.
    ///
    /// Matches [`Self::Fn`] and [`Self::ImplMethod`] only; extern signatures and trait method stubs
    /// without bodies are excluded.
    #[must_use]
    pub const fn is_function_body(self) -> bool {
        matches!(self, DefKind::Fn | DefKind::ImplMethod)
    }
}

impl Def {
    /// Builds a definition record for tests and manual table construction.
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

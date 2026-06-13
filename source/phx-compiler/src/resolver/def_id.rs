//! Definition identifiers and kinds for resolved names.
//!
//! [`DefId`] indexes [`super::ResolvedProgram::defs`]; [`DefKind`] classifies what was defined.

use phx_diagnostics::Span;
use phx_syntax::Symbol;

/// Dense index into [`crate::resolver::ResolvedProgram::defs`].
///
/// Not a stable ABI for external tools; layout may change with the compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefId(u32);

/// Error when a dense def index does not fit in `u32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DefIdOverflow;

impl DefId {
    /// Creates a definition id from a raw index (tests only).
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

    /// Returns the raw index.
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

/// What a definition represents in the source program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DefKind {
    /// Top-level or nested function.
    Fn,
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

/// One named definition in the compilation unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Def {
    /// Classification of this definition.
    pub kind: DefKind,
    /// Interned name.
    pub name: Symbol,
    /// Span of the defining name.
    pub span: Span,
    /// Owning module (program-wide id).
    pub module: u32,
    /// `true` when the item is exported (`pub` on the top-level item).
    pub exported: bool,
    /// Lexical scope depth when this binding was introduced.
    pub scope_depth: u32,
}

impl Def {
    /// Creates a definition record.
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

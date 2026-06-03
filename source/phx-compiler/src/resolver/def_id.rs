//! Definition identifiers and kinds for resolved names.
//!
//! [`DefId`] indexes [`super::ResolvedProgram::defs`]; [`DefKind`] classifies what was defined.

use phx_diagnostics::Span;
use phx_syntax::Symbol;

/// Dense index into [`crate::resolver::ResolvedProgram::defs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefId(u32);

impl DefId {
    /// Creates a definition id from a raw index (tests only).
    #[must_use]
    pub const fn from_raw(index: u32) -> Self {
        Self(index)
    }

    /// Returns the raw index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// What a definition represents in the source program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DefKind {
    /// Top-level or nested function.
    Fn,
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
    /// Owning module (crate-global id).
    pub module: u32,
    /// `true` when the item is exported (`pub` on the top-level item).
    pub exported: bool,
}

impl Def {
    /// Creates a definition record.
    #[must_use]
    pub const fn new(kind: DefKind, name: Symbol, span: Span, module: u32, exported: bool) -> Self {
        Self {
            kind,
            name,
            span,
            module,
            exported,
        }
    }
}

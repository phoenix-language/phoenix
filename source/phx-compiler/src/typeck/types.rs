//! Internal type representation and interning.

use phx_syntax::token::Keyword;

use crate::resolver::DefId;

/// Dense index into a [`TypeInterner`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(u32);

impl TypeId {
    /// Creates a type id from a raw index (tests only).
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

/// Monotonic expression index for side tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprId(u32);

impl ExprId {
    /// Creates an expression id from a raw index (internal).
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

/// A structural type in the type checker.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Ty {
    /// Primitive keyword type.
    Primitive(Keyword),
    /// Unit `()`.
    Unit,
    /// Poison type for unresolved or invalid type syntax (never a valid value type).
    Error,
    /// User-defined or generic param type.
    Named {
        /// Resolved definition.
        def: DefId,
        /// Generic arguments.
        args: Vec<TypeId>,
    },
    /// Tuple type.
    Tuple(Vec<TypeId>),
    /// Fixed array `[T; N]`.
    Array {
        /// Element type.
        elem: TypeId,
        /// Length.
        len: u32,
    },
    /// Slice `[T]`.
    Slice(TypeId),
    /// Borrow `&T` or `&mut T`.
    Ref {
        /// `true` for `&mut`.
        mut_: bool,
        /// Inner type.
        inner: TypeId,
    },
    /// Raw pointer `*T` or `*mut T`.
    Ptr {
        /// `true` for `*mut`.
        mut_: bool,
        /// Inner type.
        inner: TypeId,
    },
    /// Function type.
    Fn {
        /// Parameter types.
        params: Vec<TypeId>,
        /// Return type.
        ret: TypeId,
    },
    /// Type variable for generic inference at a call site (internal).
    Var(u32),
}

/// Intern pool of [`Ty`] values.
#[derive(Debug, Clone, Default)]
pub struct TypeInterner {
    types: Vec<Ty>,
}

/// Returns `true` when `id` is the poison [`Ty::Error`] type.
#[must_use]
pub fn is_error_type(types: &TypeInterner, id: TypeId) -> bool {
    matches!(types.get(id), Ty::Error)
}

impl TypeInterner {
    /// Creates an empty interner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Interns `ty`, returning an existing id when equal.
    #[must_use]
    pub fn intern(&mut self, ty: &Ty) -> TypeId {
        if let Some(index) = self.types.iter().position(|t| t == ty) {
            return TypeId(u32::try_from(index).unwrap_or(u32::MAX));
        }
        let index = self.types.len();
        self.types.push(ty.clone());
        TypeId(u32::try_from(index).unwrap_or(u32::MAX))
    }

    /// Borrows a type by id.
    #[must_use]
    pub fn get(&self, id: TypeId) -> &Ty {
        self.types.get(id.index() as usize).unwrap_or(&Ty::Unit)
    }

    /// All interned types.
    #[must_use]
    pub fn types(&self) -> &[Ty] {
        &self.types
    }
}

//! Internal type representation and interning.
//!
//! Defines [`Ty`], [`TypeId`], and [`TypeInterner`]. All type-checker state references types
//! through dense [`TypeId`] indices into a shared intern pool. [`ExprId`] indexes parallel
//! side tables on [`TypedProgram`](super::TypedProgram).
//!
//! # Type interning
//!
//! [`TypeInterner::intern`] deduplicates structurally equal [`Ty`] values: equal types share
//! the same [`TypeId`], so equality checks reduce to index comparison after normalization.
//! The interner grows monotonically for the lifetime of a type-check pass.
//!
//! # Poison types
//!
//! [`Ty::Error`] marks unresolved or invalid type syntax. [`TypeInterner::get`] returns
//! [`Ty::Error`] for out-of-range indices (never [`Ty::Unit`]) so internal bugs cannot
//! masquerade as unit and propagate silently through unification. Use [`is_error_type`] to
//! test for poison ids.
//!
//! # Inference variables
//!
//! [`Ty::Var`] is internal to call-site inference ([`super::infer::InferenceCtx`]). It must
//! not appear in user-visible types after checking completes.

use phx_syntax::token::Keyword;

use crate::resolver::DefId;

/// Dense index into a [`TypeInterner`] pool.
///
/// Cheap to copy and hash; two [`TypeId`] values are equal iff they refer to the same
/// interned [`Ty`] node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(u32);

impl TypeId {
    /// Creates a type id from a raw index.
    ///
    /// Intended for tests and internal reconstruction; production code should obtain ids
    /// from [`TypeInterner::intern`].
    #[must_use]
    pub const fn from_raw(index: u32) -> Self {
        Self(index)
    }

    /// Returns the raw index into the interner vector.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Monotonic expression index for side tables on [`TypedProgram`](super::TypedProgram).
///
/// Assigned sequentially while type-checking; used to attach types, move state, and other
/// per-expression metadata without embedding data in the AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprId(u32);

impl ExprId {
    /// Creates an expression id from a raw index (internal).
    #[must_use]
    pub const fn from_raw(index: u32) -> Self {
        Self(index)
    }

    /// Returns the raw side-table index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// A structural type in the type checker.
///
/// Stored in the intern pool and referenced by [`TypeId`]. Composite variants hold child
/// [`TypeId`]s rather than nested [`Ty`] values to keep nodes small and sharing explicit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Ty {
    /// Primitive keyword type.
    Primitive(Keyword),
    /// Unit `()`.
    Unit,
    /// Poison type for unresolved or invalid type syntax.
    ///
    /// Never a valid value type; diagnostics should cite the offending span rather than
    /// propagating `Error` as if it were a real type.
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
    /// UTF-8 text view `str` (`ptr`, `len`).
    Str,
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
    /// Type variable for call-site generic inference (internal).
    ///
    /// Bound and resolved by [`super::infer::InferenceCtx`]; must not survive in checked
    /// user-facing types.
    Var(u32),
}

/// Intern pool of canonical [`Ty`] values for one type-check pass.
///
/// All [`TypeId`] indices are valid for the lifetime of the interner that created them.
/// The pool is append-only; ids remain stable once allocated.
#[derive(Debug, Clone, Default)]
pub struct TypeInterner {
    types: Vec<Ty>,
}

/// Poison returned by [`TypeInterner::get`] for out-of-range [`TypeId`] indices.
static OOB_TYPE: Ty = Ty::Error;

/// Returns `true` when `id` refers to the poison [`Ty::Error`] type.
///
/// True for explicit error types and for out-of-range ids returned by [`TypeInterner::get`].
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

    /// Interns `ty`, returning an existing id when an equal type is already pooled.
    ///
    /// Structural equality uses derived [`PartialEq`] on [`Ty`]; two calls with equal
    /// structure always yield the same [`TypeId`].
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
    ///
    /// Out-of-range indices return poison [`Ty::Error`], never [`Ty::Unit`], so internal
    /// bugs cannot masquerade as unit and propagate through unification.
    ///
    /// # Panics
    ///
    /// Never panics; invalid indices yield [`Ty::Error`] via [`OOB_TYPE`].
    #[must_use]
    pub fn get(&self, id: TypeId) -> &Ty {
        self.types.get(id.index() as usize).unwrap_or(&OOB_TYPE)
    }

    /// Returns a slice of all interned types in allocation order.
    #[must_use]
    pub fn types(&self) -> &[Ty] {
        &self.types
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phx_syntax::token::Keyword;

    #[test]
    fn get_oob_on_empty_interner_returns_error_not_unit() {
        let types = TypeInterner::new();
        let oob = TypeId::from_raw(0);
        assert!(matches!(types.get(oob), Ty::Error));
        assert!(!matches!(types.get(oob), Ty::Unit));
    }

    #[test]
    fn get_large_oob_id_returns_error() {
        let mut types = TypeInterner::new();
        let _ = types.intern(&Ty::Primitive(Keyword::S32));
        let oob = TypeId::from_raw(999);
        assert!(matches!(types.get(oob), Ty::Error));
    }

    #[test]
    fn is_error_type_true_for_oob_id() {
        let types = TypeInterner::new();
        let oob = TypeId::from_raw(42);
        assert!(is_error_type(&types, oob));
    }
}

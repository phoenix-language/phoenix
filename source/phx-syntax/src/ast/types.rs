//! Type expression AST.
//!
//! Surface types: primitives, named types, generics, refs, pointers, tuples, arrays, slices, fn types.

use crate::ast::Node;
use crate::ast::ident::{Ident, TypeName};
use crate::ast::lit::IntLit;
use crate::token::Keyword;

/// A type expression.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Type {
    /// Primitive keyword type (`s32`, `bool`, …).
    Primitive(Keyword),
    /// Named type with optional generic arguments.
    Named {
        /// Type name.
        name: TypeName,
        /// Generic arguments, if any.
        generics: Option<Vec<Node<Type>>>,
    },
    /// Function type `:: (…) => T`.
    Function {
        /// Parameter types.
        params: Vec<Node<Type>>,
        /// Return type.
        ret: Box<Node<Type>>,
    },
    /// Borrow `&T` or `&mut T`.
    Ref {
        /// `true` for `&mut`.
        mut_: bool,
        /// Pointee type.
        inner: Box<Node<Type>>,
    },
    /// Raw pointer `*T` or `*mut T`.
    Ptr {
        /// `true` for `*mut`.
        mut_: bool,
        /// Pointee type.
        inner: Box<Node<Type>>,
    },
    /// Tuple type `(T, U, …)`.
    Tuple(Vec<Node<Type>>),
    /// Unit type `()`.
    Unit,
    /// Fixed array `[T; N]`.
    Array {
        /// Element type.
        elem: Box<Node<Type>>,
        /// Length literal.
        len: IntLit,
    },
    /// Slice `[T]`.
    Slice(Box<Node<Type>>),
}

/// Generic parameter `<T>` or `<T: Bound>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericParam {
    /// Parameter name.
    pub name: Ident,
    /// Optional trait bounds.
    pub bounds: Option<Vec<TypeName>>,
}

/// Generic argument list.
pub type GenericArgs = Vec<Node<Type>>;

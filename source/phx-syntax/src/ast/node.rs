//! AST node wrapper with source span and stable id.
//!
//! Every syntactic construct that needs diagnostics is wrapped in [`Node<T>`] with a [`Span`]
//! and an [`AstNodeId`] assigned during parse.
//!
//! ## Usage
//!
//! Type aliases such as [`super::expr::ExprNode`] and [`super::stmt::BlockNode`] are `Node<…>`
//! over the corresponding payload enum or struct. Identifiers ([`super::Ident`],
//! [`super::TypeName`]) carry their own span and id without an extra [`Node`] layer.
//!
//! ## Invariants
//!
//! - **Ids are assigned at parse time** and are stable for the lifetime of the tree.
//! - **Spans are half-open byte ranges** into the source string passed to [`crate::parse`].

use phx_diagnostics::Span;

use super::node_id::AstNodeId;

/// A syntax tree node carrying payload `T`, its source span, and a stable id.
#[derive(Debug, Clone, PartialEq)]
pub struct Node<T> {
    /// Node payload.
    pub inner: T,
    /// Half-open byte range in the source file.
    pub span: Span,
    /// Parse-time id for resolution and other side tables.
    pub id: AstNodeId,
}

impl<T> Node<T> {
    /// Creates a node with `inner` at `span` and `id`.
    #[must_use]
    pub const fn new(inner: T, span: Span, id: AstNodeId) -> Self {
        Self { inner, span, id }
    }
}

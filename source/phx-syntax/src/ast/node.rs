//! AST node wrapper with source span and stable id.
//!
//! Every syntactic construct that needs diagnostics is wrapped in [`Node<T>`] with a [`Span`]
//! and an [`AstNodeId`] assigned during parse.

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

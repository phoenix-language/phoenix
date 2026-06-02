//! AST node wrapper with source span.
//!
//! Every syntactic construct that needs diagnostics is wrapped in [`Node<T>`] with a [`Span`].

use phx_diagnostics::Span;

/// A syntax tree node carrying payload `T` and its source span.
#[derive(Debug, Clone, PartialEq)]
pub struct Node<T> {
    /// Node payload.
    pub inner: T,
    /// Half-open byte range in the source file.
    pub span: Span,
}

impl<T> Node<T> {
    /// Creates a node with `inner` at `span`.
    #[must_use]
    pub const fn new(inner: T, span: Span) -> Self {
        Self { inner, span }
    }
}

//! Identifier AST types.
//!
//! [`Ident`] and [`TypeName`] store [`Symbol`] indices; [`Path`] is a `::`-separated sequence.

use crate::ast::Node;
use crate::ast::node_id::AstNodeId;
use crate::intern::Symbol;
use phx_diagnostics::Span;

/// A `snake_case` identifier with its use-site span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ident {
    /// Interned name.
    pub symbol: Symbol,
    /// Source span of this identifier token.
    pub span: Span,
    /// Parse-time id for name-use resolution.
    pub id: AstNodeId,
}

/// A `PascalCase` type name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeName {
    /// Interned name.
    pub symbol: Symbol,
    /// Source span of this type name token.
    pub span: Span,
    /// Parse-time id for type-name resolution.
    pub id: AstNodeId,
}

/// A path segment in a module path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathSegment {
    /// Value/module segment (`snake_case`).
    Ident(Ident),
    /// Type segment (`PascalCase`).
    Type(TypeName),
}

/// A `::`-separated module path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Path {
    /// Path segments in order.
    pub segments: Vec<PathSegment>,
}

/// A path with span.
pub type PathNode = Node<Path>;

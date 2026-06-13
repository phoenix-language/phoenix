//! Identifier AST types.
//!
//! [`Ident`] and [`TypeName`] store [`Symbol`] indices; [`Path`] is a `::`-separated sequence.

use crate::ast::Node;
use crate::ast::node_id::AstNodeId;
use crate::ast::types::Type;
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

/// A type name in a path, optionally with generic arguments (`Foo :: <T> :: bar`).
#[derive(Debug, Clone, PartialEq)]
pub struct TypePathSegment {
    /// Type name.
    pub name: TypeName,
    /// Generic arguments on expression paths (`Foo :: <T> :: bar`).
    pub generics: Option<Vec<Node<Type>>>,
}

impl TypePathSegment {
    /// Type path segment without generic arguments.
    #[must_use]
    pub fn new(name: TypeName) -> Self {
        Self {
            name,
            generics: None,
        }
    }
}

/// A path segment in a module path.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PathSegment {
    /// Value/module segment (`snake_case`).
    Ident(Ident),
    /// Type segment (`PascalCase`).
    Type(TypePathSegment),
}

/// A `::`-separated module path.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    /// Path segments in order.
    pub segments: Vec<PathSegment>,
}

/// A path with span.
pub type PathNode = Node<Path>;

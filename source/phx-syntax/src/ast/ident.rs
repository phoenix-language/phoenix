//! Identifier AST types.

use crate::ast::Node;
use crate::intern::Symbol;

/// A `snake_case` identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ident {
    /// Interned name.
    pub symbol: Symbol,
}

/// A `PascalCase` type name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeName {
    /// Interned name.
    pub symbol: Symbol,
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

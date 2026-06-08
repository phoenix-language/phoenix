//! Pattern AST.
//!
//! Patterns for `match`, `if const` / `if var`, and bindings (wildcards, literals, struct/tuple, enum ctors).

use crate::ast::Node;
use crate::ast::expr::ExprNode;
use crate::ast::ident::{Ident, TypeName};
use crate::ast::lit::Literal;

/// A match or binding pattern.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Pattern {
    /// `_`.
    Wildcard,
    /// Literal pattern.
    Literal(Literal),
    /// Identifier pattern.
    Ident(Ident),
    /// `TypeName { … }` struct pattern.
    Struct {
        /// Enum/struct type name.
        name: TypeName,
        /// Fields: `field` or `field: pat`.
        fields: Vec<StructPatternField>,
    },
    /// `TypeName(a, b, …)` tuple variant pattern.
    Tuple {
        /// Variant/type name.
        name: TypeName,
        /// Inner patterns.
        patterns: Vec<Node<Pattern>>,
    },
}

/// Field in a struct pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct StructPatternField {
    /// Field name.
    pub name: Ident,
    /// Optional nested pattern.
    pub pattern: Option<Box<Node<Pattern>>>,
}

/// A match arm.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    /// Pattern for this arm.
    pub pattern: Node<Pattern>,
    /// Optional `if` guard.
    pub guard: Option<ExprNode>,
    /// Arm body expression or block value.
    pub body: ExprNode,
}

/// A spanned pattern.
pub type PatternNode = Node<Pattern>;

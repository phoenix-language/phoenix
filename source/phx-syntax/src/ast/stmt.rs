//! Statement and block AST.
//!
//! Statements inside blocks; blocks may end with a trailing expression value.

use crate::ast::Node;
use crate::ast::decl::ImportDirective;
use crate::ast::expr::ExprNode;
use crate::ast::ident::Ident;
use crate::ast::types::Type;

/// A statement.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Stmt {
    /// `const name [: T] = expr;`
    Const {
        /// Binding name.
        name: Ident,
        /// Optional type annotation.
        ty: Option<Node<Type>>,
        /// Initializer.
        init: ExprNode,
    },
    /// `var name: T = expr;`
    Var {
        /// Binding name.
        name: Ident,
        /// Type annotation.
        ty: Node<Type>,
        /// Initializer.
        init: ExprNode,
    },
    /// Assignment statement.
    Assign {
        /// Assignment expression (includes target and value).
        expr: ExprNode,
    },
    /// Expression statement.
    Expr(ExprNode),
    /// `return [expr];`
    Return(Option<ExprNode>),
    /// `break [expr];`
    Break {
        /// Optional value expression.
        value: Option<ExprNode>,
        /// Span of the `break` keyword.
        span: phx_diagnostics::Span,
    },
    /// `continue;`
    Continue {
        /// Span of the `continue` keyword.
        span: phx_diagnostics::Span,
    },
    /// `while cond { … }`
    While {
        /// Loop condition.
        cond: ExprNode,
        /// Loop body.
        body: BlockNode,
    },
    /// `for binding in iter { … }`
    ForIn {
        /// Loop binding.
        binding: Ident,
        /// Iterable expression.
        iter: ExprNode,
        /// Loop body.
        body: BlockNode,
    },
    /// `loop { … }`
    Loop(BlockNode),
    /// `#unsafe { … }`
    Unsafe(BlockNode),
}

/// Item inside a block: statement or trailing expression.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum BlockItem {
    /// Statement with semicolon.
    Stmt(StmtNode),
    /// Trailing expression without semicolon.
    Expr(ExprNode),
    /// Block-scoped `#import` directive.
    Import(Node<ImportDirective>),
}

/// A `{ … }` block.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Items in source order.
    pub items: Vec<BlockItem>,
}

/// A spanned block.
pub type BlockNode = Node<Block>;
/// A spanned statement.
pub type StmtNode = Node<Stmt>;

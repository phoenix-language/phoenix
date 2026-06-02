//! Expression AST.
//!
//! Covers literals through assignment, casts, postfix chains, and control-flow expressions.

use crate::ast::Node;
use crate::ast::ident::{Ident, Path, TypeName};
use crate::ast::lit::Literal;
use crate::ast::pat::MatchArm;
use crate::ast::stmt::BlockNode;
use crate::ast::types::Type;

/// Binary operators on expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BinOp {
    /// `||`
    Or,
    /// `&&`
    And,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `|`
    BitOr,
    /// `^`
    BitXor,
    /// `&`
    BitAnd,
    /// `<<`
    Shl,
    /// `>>`
    Shr,
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Mod,
    /// `**`
    Pow,
}

/// Unary operators on expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum UnaryOp {
    /// `-`
    Neg,
    /// `!`
    Not,
    /// `~`
    BitNot,
    /// `*` deref
    Deref,
    /// `&`
    Ref,
    /// `&mut`
    RefMut,
}

/// Assignment operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AssignOp {
    /// `=`
    Assign,
    /// `+=`
    AddAssign,
    /// `-=`
    SubAssign,
    /// `*=`
    MulAssign,
    /// `/=`
    DivAssign,
    /// `%=`
    ModAssign,
}

/// Postfix operations applied left-to-right.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PostfixOp {
    /// `.field`
    Field(Ident),
    /// `.method(args)`
    Method {
        /// Method name.
        name: Ident,
        /// Generic args on method, if any.
        generics: Option<Vec<Node<Type>>>,
        /// Call arguments.
        args: Vec<ExprNode>,
    },
    /// `(args)`
    Call(Vec<ExprNode>),
    /// `[index]`
    Index(ExprNode),
    /// `?`
    Try,
}

/// An expression.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Expr {
    /// Literal value.
    Literal(Literal),
    /// Identifier use.
    Ident(Ident),
    /// Module path use.
    Path(Path),
    /// Parenthesized or tuple expression.
    Tuple(Vec<ExprNode>),
    /// Array literal `[…]`.
    Array(Vec<ExprNode>),
    /// Unary operation.
    Unary {
        /// Operator.
        op: UnaryOp,
        /// Operand.
        operand: Box<ExprNode>,
    },
    /// Binary operation (left-associative at each precedence level).
    Binary {
        /// Operator.
        op: BinOp,
        /// Left operand.
        left: Box<ExprNode>,
        /// Right operand.
        right: Box<ExprNode>,
    },
    /// Assignment expression.
    Assign {
        /// Operator.
        op: AssignOp,
        /// Target.
        target: Box<ExprNode>,
        /// Value.
        value: Box<ExprNode>,
    },
    /// Cast with `as`.
    Cast {
        /// Expression.
        expr: Box<ExprNode>,
        /// Target type.
        ty: Box<Node<Type>>,
    },
    /// Postfix chain on a primary.
    Postfix {
        /// Primary expression.
        base: Box<ExprNode>,
        /// Postfix operations in order.
        ops: Vec<PostfixOp>,
    },
    /// `if` expression.
    If {
        /// Condition.
        cond: Box<ExprNode>,
        /// Then block.
        then_block: BlockNode,
        /// `else if` / `else` arms.
        else_ifs: Vec<(ExprNode, BlockNode)>,
        /// Final `else` block.
        else_block: Option<BlockNode>,
    },
    /// `match` expression.
    Match {
        /// Scrutinee.
        scrutinee: Box<ExprNode>,
        /// Match arms.
        arms: Vec<MatchArm>,
    },
    /// Block used as expression.
    Block(BlockNode),
    /// Struct literal `Type { … }`.
    StructLit {
        /// Type name.
        name: TypeName,
        /// Generic args.
        generics: Option<Vec<Node<Type>>>,
        /// Field initializers.
        fields: Vec<StructFieldInit>,
    },
    /// `#unsafe` block expression.
    Unsafe(BlockNode),
    /// `Some(x)`, `None`, `Ok(x)`, `Err(x)` constructor expression.
    EnumCtor {
        /// `Some`, `None`, `Ok`, or `Err`.
        variant: crate::token::Keyword,
        /// Inner expression for `Some`/`Ok`/`Err`; `None` has no inner.
        inner: Option<Box<ExprNode>>,
    },
}

/// Struct literal field `field: expr` or `..base`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum StructFieldInit {
    /// `field: expr`.
    Field {
        /// Field name.
        name: Ident,
        /// Value expression.
        value: ExprNode,
    },
    /// `..base` functional update.
    Spread(ExprNode),
}

/// A spanned expression.
pub type ExprNode = Node<Expr>;

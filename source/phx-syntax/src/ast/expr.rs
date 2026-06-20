//! Expression AST.
//!
//! Covers literals through assignment, casts, postfix chains, and control-flow expressions.
//! The main payload is [`Expr`]; spanned nodes use [`ExprNode`] (`Node<Expr>`).
//!
//! ## Operator enums
//!
//! - [`BinOp`], [`UnaryOp`], [`AssignOp`] — infix, prefix, and assignment operators.
//! - [`PostfixOp`] — field access, method/call chains, indexing, and `?`.
//!
//! ## Control flow
//!
//! - [`IfCondition`] — boolean `if` or `if const` / `if var` pattern bindings.
//! - [`MatchArm`] (in [`crate::pat`]) — pattern plus block or expression body.
//! - [`LambdaBody`] — closure body after `=>` (expression or block).
//!
//! ## Post-MVP surface
//!
//! [`RuntimeDirectiveKind`] and [`Expr::RuntimeDirective`] parse `@spawn` / `@send` syntax reserved
//! for a future runtime; the MVP compiler may reject them later in the pipeline.

use crate::ast::Node;
use crate::ast::decl::Param;
use crate::ast::ident::{Ident, Path, TypeName};
use crate::ast::lit::Literal;
use crate::ast::pat::{MatchArm, PatternNode};
use crate::ast::stmt::BlockNode;
use crate::ast::types::Type;

/// Binary operators on expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// `(args)` with optional leading `:: < … >`.
    Call {
        /// Type arguments before `(` when present.
        generics: Option<Vec<Node<Type>>>,
        /// Call arguments.
        args: Vec<ExprNode>,
    },
    /// `[index]`
    Index(ExprNode),
    /// `?`
    Try,
}

/// `if` condition: boolean expression or `const` / `var` pattern binding.
#[derive(Debug, Clone, PartialEq)]
pub enum IfCondition {
    /// `if expr { … }`
    Bool(ExprNode),
    /// `if const pat = expr { … }` or `if var pat = expr { … }`
    Pattern {
        /// `true` for `if var`, `false` for `if const`.
        mutable: bool,
        /// Pattern to match.
        pattern: PatternNode,
        /// Scrutinee expression.
        scrutinee: ExprNode,
    },
}

/// Post-MVP runtime directive (`@spawn`, `@send`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeDirectiveKind {
    /// `@spawn(expr)`
    Spawn,
    /// `@send(expr, expr)`
    Send,
    /// `@receive(expr)`
    Receive,
    /// `@reply(expr)`
    Reply,
}

/// Body of a lambda expression.
#[derive(Debug, Clone, PartialEq)]
pub enum LambdaBody {
    /// Single expression after `=>`.
    Expr(Box<ExprNode>),
    /// Block after `=>`.
    Block(BlockNode),
}

/// An expression.
#[derive(Debug, Clone, PartialEq)]
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
    /// `if` expression (`if cond`, `if const pat = e`, or `if var pat = e`).
    If {
        /// Condition or pattern binding.
        condition: Box<IfCondition>,
        /// Then block.
        then_block: BlockNode,
        /// `else if` arms.
        else_ifs: Vec<(IfCondition, BlockNode)>,
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
    /// Range `start..end` or `start..=end`.
    Range {
        /// Left bound.
        start: Box<ExprNode>,
        /// Right bound.
        end: Box<ExprNode>,
        /// `true` for `..=`.
        inclusive: bool,
    },
    /// Closure `(params) => body`.
    Lambda {
        /// Parameters.
        params: Vec<Param>,
        /// Body expression or block.
        body: LambdaBody,
    },
    /// Runtime directive expression (`@spawn`, …).
    RuntimeDirective {
        /// Which directive.
        kind: RuntimeDirectiveKind,
        /// Arguments (arity depends on `kind`).
        args: Vec<ExprNode>,
    },
}

/// Struct literal field `field: expr` or `..base`.
#[derive(Debug, Clone, PartialEq)]
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

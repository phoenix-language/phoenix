//! Phoenix abstract syntax tree.
//!
//! Plain-data syntax tree produced by [`crate::parser`] and consumed by resolver and type-check
//! passes in `phx-compiler`. Nodes carry [`Span`](phx_diagnostics::Span) and [`AstNodeId`] for
//! diagnostics and side tables; they do not carry semantic types or resolved symbols.
//!
//! ## Submodules
//!
//! - [`decl`] — [`Program`], functions, structs, enums, traits, impls, imports.
//! - [`expr`] — expressions, operators, struct literals, `if`/`match`.
//! - [`stmt`] — statements and [`Block`] items.
//! - [`pat`] — `match` / `if` pattern bindings and arms.
//! - [`types`] — type expressions and generic parameters.
//! - [`ident`] — [`Ident`], [`TypeName`], and [`Path`] segments.
//! - [`lit`] — literal payloads (int, float, bool, byte string).
//! - [`attr`] — bracket item attributes (`#[name(…)]`).
//! - [`node`] — [`Node<T>`] span wrapper.
//! - [`node_id`] — [`AstNodeId`] assigned at parse time.
//!
//! ## Invariants
//!
//! - **No analysis on nodes.** AST types are records and enums only — no methods that resolve,
//!   type-check, or transform the tree.
//! - **Spanned nodes use [`Node<T>`].** Payload plus [`Span`](phx_diagnostics::Span) plus
//!   [`AstNodeId`].
//! - **Names use [`crate::Symbol`].** [`Ident`] and [`TypeName`] store interned indices, not
//!   heap strings.
//! - **Partial trees are valid after error recovery.** The parser may return a [`Program`] with
//!   holes; downstream passes must consult [`ParseResult::has_errors`](crate::ParseResult).

pub mod attr;
pub mod decl;
pub mod expr;
pub mod ident;
pub mod lit;
pub mod node;
pub mod node_id;
pub mod pat;
pub mod stmt;
pub mod types;

pub use attr::{AttrArg, AttrValue, Attribute};
pub use decl::{
    EnumVariant, FnDirective, Function, FunctionSig, ImplMember, ImportDirective, ImportItem,
    ImportItems, Param, Program, StructBody, StructField, TopLevelDecl, TopLevelItem, TraitItem,
    Variant,
};
pub use expr::{AssignOp, BinOp, Expr, ExprNode, IfCondition, PostfixOp, StructFieldInit, UnaryOp};
pub use ident::{Ident, Path, PathSegment, TypeName, TypePathSegment};
pub use lit::{FloatLit, IntLit, Literal};
pub use node::Node;
pub use node_id::AstNodeId;
pub use pat::{MatchArm, Pattern, PatternNode, StructPatternField};
pub use stmt::{Block, BlockItem, BlockNode, Stmt, StmtNode};
pub use types::{GenericParam, Type};

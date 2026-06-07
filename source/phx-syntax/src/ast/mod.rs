//! Phoenix abstract syntax tree.
//!
//! Nodes are plain data with [`Node`] spans. Identifiers use [`crate::Symbol`], not `String`.
//!
//! ## Submodules
//!
//! - [`decl`] — `Program`, functions, structs, enums, traits, impls, imports.
//! - [`expr`] — expressions, operators, struct literals, `if`/`match`.
//! - [`stmt`] — statements and [`Block`] items.
//! - [`pat`] — `match` / `given` patterns and arms.
//! - [`types`] — type expressions and generic parameters.
//! - [`ident`] — [`Ident`], [`TypeName`], and [`Path`] segments.
//! - [`lit`] — literal payloads (int, float, bool, byte string).
//! - [`node`] — [`Node<T>`] span wrapper.
//! - [`node_id`] — [`AstNodeId`] assigned at parse time.

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
pub use expr::{AssignOp, BinOp, Expr, ExprNode, PostfixOp, StructFieldInit, UnaryOp};
pub use ident::{Ident, Path, PathSegment, TypeName};
pub use lit::{FloatLit, IntLit, Literal};
pub use node::Node;
pub use node_id::AstNodeId;
pub use pat::{MatchArm, Pattern, PatternNode, StructPatternField};
pub use stmt::{Block, BlockItem, BlockNode, Stmt, StmtNode};
pub use types::{GenericParam, Type};

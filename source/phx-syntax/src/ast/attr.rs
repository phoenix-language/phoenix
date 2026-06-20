//! Item attribute AST (`#[name(...)]`).
//!
//! Bracket attributes prefix declarations and functions. Parsed into [`Attribute`] nodes before
//! the item body; [`crate::attr_collect`] gathers them for resolve and codegen passes.
//!
//! ## Argument shapes
//!
//! - [`AttrArg::Named`] — `key = value`.
//! - [`AttrArg::Flag`] — bare identifier flag.
//! - [`AttrArg::Nested`] — nested calls such as `not(…)` / `all(…)` in `#[cfg(…)]`.
//! - [`AttrArg::TypeName`] — positional type name (e.g. traits in `#[derive(…)]`).
//!
//! [`AttrValue`] is string, identifier, or boolean inside named arguments.

use crate::ast::ident::{Ident, TypeName};

/// One compile-time item attribute.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    /// Attribute name (`cfg`, `deprecated`, `allow`, …).
    pub name: Ident,
    /// Parenthesized arguments, if any.
    pub args: Vec<AttrArg>,
}

/// One argument inside `#[name(...)]`.
#[derive(Debug, Clone, PartialEq)]
pub enum AttrArg {
    /// `key = value`.
    Named {
        /// Argument name.
        name: Ident,
        /// Argument value.
        value: AttrValue,
    },
    /// Flag without value (`debug_assertions`, `must_use` with no parens uses empty args).
    Flag(Ident),
    /// Nested call (`not(target_os = "linux")`).
    Nested {
        /// Nested name (`not`, `all`, …).
        name: Ident,
        /// Inner arguments.
        args: Vec<AttrArg>,
    },
    /// Positional type name (`#[derive(Debug, PartialEq)]`).
    TypeName(TypeName),
}

/// Attribute argument value.
#[derive(Debug, Clone, PartialEq)]
pub enum AttrValue {
    /// String literal.
    Str(String),
    /// Identifier (`true` / `false` or symbolic name).
    Ident(Ident),
    /// Boolean literal.
    Bool(bool),
}

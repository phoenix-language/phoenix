//! Structural helpers to gather bracket `#[…]` attributes from AST nodes.
//!
//! Phoenix item attributes (`#[inline]`, `#[cfg(…)]`, `#[deprecated]`, …) are parsed into
//! [`Attribute`](crate::ast::attr::Attribute) nodes and stored on declarations and functions.
//! Later passes (resolve, type check, codegen) need a consistent view of which attributes apply
//! to a top-level item versus its nested function body — this module provides that aggregation
//! without re-walking the parser.
//!
//! ## Attribute placement
//!
//! | Node | Where attributes live |
//! |------|------------------------|
//! | Top-level item | [`TopLevelItem::attrs`](crate::ast::decl::TopLevelItem::attrs) |
//! | Function / method body | [`Function::attrs`](crate::ast::decl::Function::attrs) on the inner [`Function`](crate::ast::decl::Function) |
//!
//! For a top-level function, attributes may appear on both the wrapping item and the function
//! struct (for example `#[allow(…)]` on the item and `#[inline]` on the fn). Use
//! [`top_level_bracket_attrs`] to collect both; use [`function_bracket_attrs`] when you already
//! hold a [`Function`] reference (impl methods, nested fns).
//!
//! ## Public API
//!
//! - [`top_level_bracket_attrs`] — item-level plus function-level attrs for one top-level item.
//! - [`function_bracket_attrs`] — attrs on a [`Function`] node only.
//!
//! ## Pipeline position
//!
//! Read-only queries over the untyped AST after [`crate::parse`]. Does not evaluate `#[cfg]`
//! or validate attribute names.

use crate::ast::Node;
use crate::ast::attr::Attribute;
use crate::ast::decl::{Function, TopLevelDecl, TopLevelItem};

/// Returns all bracket attributes on a top-level item, including nested function attrs.
///
/// Item-level attributes ([`TopLevelItem::attrs`](crate::ast::decl::TopLevelItem::attrs)) are
/// returned first in source order, followed by attributes on the inner [`Function`] when the
/// item is a function declaration. Non-function items (structs, enums, traits, consts) return
/// only item-level attributes.
///
/// # Panics
///
/// Never panics — read-only slice collection from parsed AST nodes.
///
/// # Examples
///
/// ```
/// use phx_syntax::{parse, top_level_bracket_attrs};
///
/// let src = "#[allow(dead_code)]\nmain :: () => { };";
/// let file = parse(src);
/// assert!(!file.has_errors());
/// let item = &file.value.program.items[0].inner;
/// assert_eq!(top_level_bracket_attrs(item).len(), 1);
/// ```
#[must_use]
pub fn top_level_bracket_attrs(item: &TopLevelItem) -> Vec<&Node<Attribute>> {
    let mut out: Vec<&Node<Attribute>> = item.attrs.iter().collect();
    if let TopLevelDecl::Function(f) = &item.decl {
        out.extend(f.attrs.iter());
    }
    out
}

/// Returns bracket attributes attached directly to a function node.
///
/// Covers top-level functions and impl methods. Does **not** include attributes on the
/// wrapping [`TopLevelItem`] — use [`top_level_bracket_attrs`] when you need the full set for
/// a file-level declaration.
///
/// # Panics
///
/// Never panics — returns a slice view into the parsed AST.
///
/// # Examples
///
/// Impl methods store attributes on the [`Function`] node (top-level fn attrs live on
/// [`TopLevelItem::attrs`](crate::ast::decl::TopLevelItem::attrs) instead):
///
/// ```
/// use phx_syntax::{function_bracket_attrs, parse};
///
/// let src = r"
/// Point :: struct { x: s32, y: s32 };
///
/// Point :: impl {
///   #[inline]
///   sum :: (self) => { self.x + self.y };
/// };
///
/// main :: () => { };
/// ";
/// let file = parse(src);
/// assert!(!file.has_errors());
/// let impl_item = file
///     .value
///     .program
///     .items
///     .iter()
///     .find(|i| matches!(i.inner.decl, phx_syntax::ast::decl::TopLevelDecl::Impl { .. }))
///     .expect("impl item");
/// if let phx_syntax::ast::decl::TopLevelDecl::Impl { members, .. } = &impl_item.inner.decl {
///     if let phx_syntax::ast::decl::ImplMember::Method(f) = &members[0] {
///         assert_eq!(function_bracket_attrs(f).len(), 1);
///     }
/// }
/// ```
#[must_use]
pub fn function_bracket_attrs(func: &Function) -> &[Node<Attribute>] {
    &func.attrs
}

//! Structural helpers to gather bracket attributes from AST nodes.

use crate::ast::Node;
use crate::ast::attr::Attribute;
use crate::ast::decl::{Function, TopLevelDecl, TopLevelItem};

/// Returns all bracket attributes on a top-level item (including nested function attrs).
#[must_use]
pub fn top_level_bracket_attrs(item: &TopLevelItem) -> Vec<&Node<Attribute>> {
    let mut out: Vec<&Node<Attribute>> = item.attrs.iter().collect();
    if let TopLevelDecl::Function(f) = &item.decl {
        out.extend(f.attrs.iter());
    }
    out
}

/// Returns bracket attributes on a function (impl methods and top-level fns).
#[must_use]
pub fn function_bracket_attrs(func: &Function) -> &[Node<Attribute>] {
    &func.attrs
}

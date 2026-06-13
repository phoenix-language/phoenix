//! Definition metadata from item attributes.

use std::collections::HashMap;

use phx_syntax::ast::Node;
use phx_syntax::ast::attr::Attribute;
use phx_syntax::ast::decl::{Function, TopLevelItem};
use phx_syntax::{
    DeprecatedMeta, Interner, allow_names_from_attrs, deprecated_from_attrs, has_must_use_attr,
    top_level_bracket_attrs,
};

use crate::resolver::DefId;

/// Metadata attached to a definition from `#[...]` attributes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemAttrs {
    /// `#[deprecated(...)]` when present.
    pub deprecated: Option<DeprecatedMeta>,
    /// `#[must_use]` when present.
    pub must_use: bool,
}

/// Map from definition id to item attribute metadata.
pub type DefAttrs = HashMap<DefId, ItemAttrs>;

/// Builds [`ItemAttrs`] from bracket attributes on a top-level item.
#[must_use]
pub fn item_attrs_for_top_level(interner: &Interner, item: &TopLevelItem) -> ItemAttrs {
    let refs = top_level_bracket_attrs(item);
    let owned: Vec<Node<Attribute>> = refs.into_iter().map(|n| (*n).clone()).collect();
    let mut attrs = item_attrs_from_bracket(interner, &owned);
    if let phx_syntax::ast::decl::TopLevelDecl::Function(f) = &item.decl {
        let fn_attrs = item_attrs_from_bracket(interner, &f.attrs);
        attrs.must_use |= fn_attrs.must_use;
        if attrs.deprecated.is_none() {
            attrs.deprecated = fn_attrs.deprecated;
        }
    }
    attrs
}

/// Builds [`ItemAttrs`] from bracket attributes on a function.
#[must_use]
pub fn item_attrs_for_function(interner: &Interner, func: &Function) -> ItemAttrs {
    item_attrs_from_bracket(interner, &func.attrs)
}

/// Builds [`ItemAttrs`] from a bracket attribute list.
#[must_use]
pub fn item_attrs_from_bracket(interner: &Interner, attrs: &[Node<Attribute>]) -> ItemAttrs {
    ItemAttrs {
        deprecated: deprecated_from_attrs(interner, attrs),
        must_use: has_must_use_attr(interner, attrs),
    }
}

/// Parses `#[allow(...)]` names into [`LintKind`] values; returns errors for unknown names.
pub fn parse_allow_lint_kinds(
    interner: &Interner,
    attrs: &[Node<Attribute>],
) -> Result<Vec<phx_diagnostics::LintKind>, String> {
    let mut kinds = Vec::new();
    for sym in allow_names_from_attrs(interner, attrs) {
        match interner.resolve(sym) {
            Some("deprecated") => kinds.push(phx_diagnostics::LintKind::Deprecated),
            Some("must_use") => kinds.push(phx_diagnostics::LintKind::MustUse),
            Some(other) => {
                return Err(format!("unknown lint name `{other}` in `#[allow(...)]`"));
            }
            None => {
                return Err(format!(
                    "unknown lint name `sym#{}` in `#[allow(...)]`",
                    sym.index()
                ));
            }
        }
    }
    Ok(kinds)
}

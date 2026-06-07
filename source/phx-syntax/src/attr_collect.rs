//! Helpers to gather bracket and keyword attributes from AST nodes.

use crate::Symbol;
use crate::ast::Node;
use crate::ast::attr::{AttrArg, Attribute};
use crate::ast::decl::{DeriveDirective, Function, TopLevelDecl, TopLevelItem};
use crate::intern::Interner;

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

/// Merges `#[derive(...)]` traits from bracket attributes into a derive list.
#[must_use]
pub fn derive_from_bracket_attrs(
    interner: &Interner,
    attrs: &[Node<Attribute>],
) -> Vec<DeriveDirective> {
    let mut out = Vec::new();
    for attr in attrs {
        if interner.resolve(attr.inner.name.symbol) != "derive" {
            continue;
        }
        let mut traits = Vec::new();
        for arg in &attr.inner.args {
            if let AttrArg::TypeName(ty) = arg {
                traits.push(*ty);
            }
        }
        if !traits.is_empty() {
            out.push(DeriveDirective { traits });
        }
    }
    out
}

/// Returns whether an attribute list contains `#[must_use]`.
#[must_use]
pub fn has_must_use_attr(interner: &Interner, attrs: &[Node<Attribute>]) -> bool {
    attrs
        .iter()
        .any(|a| interner.resolve(a.inner.name.symbol) == "must_use" && a.inner.args.is_empty())
}

/// Returns `true` when `name` matches the attribute identifier (via interner).
#[must_use]
pub fn attr_named(interner: &Interner, attr: &Attribute, name: &str) -> bool {
    interner.resolve(attr.name.symbol) == name
}

/// Collects deprecated metadata from `#[deprecated(...)]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeprecatedMeta {
    /// `since = "…"` when present.
    pub since: Option<String>,
    /// `note = "…"` when present.
    pub note: Option<String>,
    /// `suggestion = "…"` when present.
    pub suggestion: Option<String>,
}

/// Parses `#[deprecated(...)]` from bracket attributes, if any.
#[must_use]
pub fn deprecated_from_attrs(
    interner: &Interner,
    attrs: &[Node<Attribute>],
) -> Option<DeprecatedMeta> {
    for attr in attrs {
        if interner.resolve(attr.inner.name.symbol) != "deprecated" {
            continue;
        }
        let mut meta = DeprecatedMeta::default();
        for arg in &attr.inner.args {
            if let AttrArg::Named { name, value } = arg {
                let key = interner.resolve(name.symbol);
                match (key, value) {
                    ("since", crate::ast::AttrValue::Str(s)) => meta.since = Some(s.clone()),
                    ("note", crate::ast::AttrValue::Str(s)) => meta.note = Some(s.clone()),
                    ("suggestion", crate::ast::AttrValue::Str(s)) => {
                        meta.suggestion = Some(s.clone());
                    }
                    _ => {}
                }
            }
        }
        return Some(meta);
    }
    None
}

/// Allow lint names from `#[allow(...)]` on the given attributes.
#[must_use]
pub fn allow_names_from_attrs(interner: &Interner, attrs: &[Node<Attribute>]) -> Vec<Symbol> {
    let mut names = Vec::new();
    for attr in attrs {
        if interner.resolve(attr.inner.name.symbol) != "allow" {
            continue;
        }
        for arg in &attr.inner.args {
            match arg {
                AttrArg::Flag(ident) => names.push(ident.symbol),
                AttrArg::Named { name, .. } => names.push(name.symbol),
                _ => {}
            }
        }
    }
    names
}

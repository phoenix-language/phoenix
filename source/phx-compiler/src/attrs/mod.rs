//! Bracket-attribute parsing and definition metadata for the compiler front end.
//!
//! Parses `#[...]` attributes attached to AST items and builds metadata consumed by later
//! passes: derive lists, lint suppression, and per-definition attribute records.
//!
//! ## Supported attributes
//!
//! | Attribute | Role in this module |
//! |-----------|---------------------|
//! | `#[derive(...)]` | Merged into [`DeriveDirective`] lists before [`crate::derive::expand_derives`] |
//! | `#[deprecated(...)]` | Stored on [`ItemAttrs`] and surfaced by the lint pass |
//! | `#[must_use]` | Stored on [`ItemAttrs`] and surfaced by the lint pass |
//! | `#[allow(...)]` | Parsed into [`phx_diagnostics::LintKind`] values for lexical suppression |
//!
//! Other attributes (`#[cfg(...)]`, `#[lang_item]`, …) are handled by sibling modules.
//!
//! ## Pipeline position
//!
//! 1. [`merge_bracket_derives_into_program`] runs in the module loader and single-file
//!    compile path immediately after parse, before [`crate::cfg::strip_cfg`].
//! 2. During resolution, [`item_attrs_for_top_level`] and [`item_attrs_for_function`] build
//!    [`ItemAttrs`] for each definition; results are stored in [`DefAttrs`] on
//!    [`crate::resolver::ResolvedProgram`].
//! 3. The lint pass reads [`DefAttrs`] and calls [`parse_allow_lint_kinds`] to honor
//!    `#[allow(...)]` on module items and function bodies.

use std::collections::HashMap;

use phx_syntax::Symbol;
use phx_syntax::ast::Node;
use phx_syntax::ast::attr::{AttrArg, Attribute};
use phx_syntax::ast::decl::{DeriveDirective, Function, Program, TopLevelDecl, TopLevelItem};
use phx_syntax::{Interner, attr_collect::top_level_bracket_attrs};

use crate::resolver::DefId;

/// Metadata attached to a definition from `#[...]` attributes.
///
/// Collected during resolution and read by the lint pass. Only attributes that affect
/// downstream compiler behavior are represented here; unknown attributes are ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemAttrs {
    /// `#[deprecated(...)]` when present.
    pub deprecated: Option<DeprecatedMeta>,
    /// `#[must_use]` when present.
    pub must_use: bool,
}

/// `#[deprecated(...)]` fields parsed from bracket attributes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeprecatedMeta {
    /// `since = "…"` when present.
    pub since: Option<String>,
    /// `note = "…"` when present.
    pub note: Option<String>,
    /// `suggestion = "…"` when present.
    pub suggestion: Option<String>,
}

/// Map from definition id to item attribute metadata.
///
/// Populated by the resolver walk and consumed by [`crate::lint::lint_program`].
pub type DefAttrs = HashMap<DefId, ItemAttrs>;

/// Merges `#[derive(...)]` from bracket attributes into each item's derive list.
///
/// Top-level item attributes and nested function attributes are both scanned so derive
/// directives written in bracket form match those written in declaration syntax.
///
/// Called before [`crate::derive::expand_derives`] in the module loader and
/// [`crate::compile::compile_source`].
pub fn merge_bracket_derives_into_program(program: &mut Program, interner: &Interner) {
    for item in &mut program.items {
        merge_bracket_derives_into_decl(&mut item.inner.decl, &item.inner.attrs, interner);
        if let TopLevelDecl::Function(f) = &mut item.inner.decl {
            let extra = derive_from_bracket_attrs(interner, &f.attrs);
            f.derives.extend(extra);
        }
    }
}

fn merge_bracket_derives_into_decl(
    decl: &mut TopLevelDecl,
    attrs: &[Node<Attribute>],
    interner: &Interner,
) {
    let extra = derive_from_bracket_attrs(interner, attrs);
    if extra.is_empty() {
        return;
    }
    match decl {
        TopLevelDecl::Struct { derives, .. }
        | TopLevelDecl::Enum { derives, .. }
        | TopLevelDecl::Trait { derives, .. } => derives.extend(extra),
        TopLevelDecl::Function(f) => f.derives.extend(extra),
        _ => {}
    }
}

/// Collects `#[derive(...)]` traits from bracket attributes into a derive list.
///
/// Each `#[derive(A, B)]` becomes one [`DeriveDirective`] with the listed type names.
/// Non-type arguments are skipped.
#[must_use]
pub fn derive_from_bracket_attrs(
    interner: &Interner,
    attrs: &[Node<Attribute>],
) -> Vec<DeriveDirective> {
    let mut out = Vec::new();
    for attr in attrs {
        if !attr_named(interner, &attr.inner, "derive") {
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
        .any(|a| attr_named(interner, &a.inner, "must_use") && a.inner.args.is_empty())
}

/// Returns `true` when `name` matches the attribute identifier (via interner).
#[must_use]
pub fn attr_named(interner: &Interner, attr: &Attribute, name: &str) -> bool {
    interner.resolves_to(attr.name.symbol, name)
}

/// Parses `#[deprecated(...)]` from bracket attributes, if any.
///
/// Returns the first matching attribute; later `#[deprecated]` attributes are ignored.
#[must_use]
pub fn deprecated_from_attrs(
    interner: &Interner,
    attrs: &[Node<Attribute>],
) -> Option<DeprecatedMeta> {
    for attr in attrs {
        if !attr_named(interner, &attr.inner, "deprecated") {
            continue;
        }
        let mut meta = DeprecatedMeta::default();
        for arg in &attr.inner.args {
            if let AttrArg::Named { name, value } = arg {
                let key = interner.resolve(name.symbol);
                match (key, value) {
                    (Some("since"), phx_syntax::ast::AttrValue::Str(s)) => {
                        meta.since = Some(s.clone());
                    }
                    (Some("note"), phx_syntax::ast::AttrValue::Str(s)) => {
                        meta.note = Some(s.clone());
                    }
                    (Some("suggestion"), phx_syntax::ast::AttrValue::Str(s)) => {
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
///
/// Accepts flag form (`#[allow(deprecated)]`) and named form (`#[allow(deprecated = …)]`);
/// only the lint name symbol is collected.
#[must_use]
pub fn allow_names_from_attrs(interner: &Interner, attrs: &[Node<Attribute>]) -> Vec<Symbol> {
    let mut names = Vec::new();
    for attr in attrs {
        if !attr_named(interner, &attr.inner, "allow") {
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

/// Builds [`ItemAttrs`] from bracket attributes on a top-level item.
///
/// Merges the item's outer attributes with nested function attributes when the item is a
/// function declaration (`must_use` is OR-ed; first `deprecated` wins on the outer attrs).
#[must_use]
pub fn item_attrs_for_top_level(interner: &Interner, item: &TopLevelItem) -> ItemAttrs {
    let refs = top_level_bracket_attrs(item);
    let owned: Vec<Node<Attribute>> = refs.into_iter().map(|n| (*n).clone()).collect();
    let mut attrs = item_attrs_from_bracket(interner, &owned);
    if let TopLevelDecl::Function(f) = &item.decl {
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

/// Parses `#[allow(...)]` names into [`LintKind`] values.
///
/// # Errors
///
/// Returns an error string when an allow name is not a known lint (`deprecated`, `must_use`).
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

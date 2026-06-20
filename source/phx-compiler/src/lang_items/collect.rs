//! Collect `#[lang_item]` markers into a [`LangItemRegistry`].
//!
//! Walks every module in a [`ResolvedProgram`](crate::resolver::ResolvedProgram), parses
//! `#[lang_item]` attributes on std definitions, merges markers rehydrated from dependency
//! `.pxi` import metadata, validates each pair against the closed v1 registry, and auto-links
//! sibling variant ctors for registered `Option`/`Result` enum templates.
//!
//! ## Pipeline position
//!
//! [`build_lang_item_registry`] is invoked at the start of type checking (see
//! `typeck::check::decl`) after name resolution. Its output is stored on
//! [`TypedProgram`](crate::typeck::TypedProgram) and consulted by expression checking,
//! monomorphization, lowering, and PXI export.
//!
//! ## Owning pass
//!
//! - **Type checking (setup)** — builds the registry once per program; diagnostics are
//!   collected into a [`TypeCheckBag`](phx_diagnostics::TypeCheckBag) without aborting
//!   collection when individual markers fail validation.
//!
//! ## Sources scanned
//!
//! - Std source items under the `std::` logical module tree (`#[lang_item]` on functions,
//!   enums, and traits).
//! - [`ResolvedProgram::import_lang_items`](crate::resolver::ResolvedProgram) entries from
//!   linked dependency `.pxi` files (fill gaps when the current program does not define the
//!   same `(kind, name)` locally).
//!
//! User modules may not declare language items — attempts produce
//! [`TypeCheckError::LangItemReserved`](crate::typeck::TypeCheckError::LangItemReserved).
//!
//! ## In this module
//!
//! - [`build_lang_item_registry`] — main entry: scan, validate, deduplicate, auto-link variants.

use phx_diagnostics::{Span, TypeCheckBag, TypeCheckError};
use phx_syntax::Interner;
use phx_syntax::ast::Node;
use phx_syntax::ast::decl::{TopLevelDecl, TopLevelItem};
use phx_syntax::{function_bracket_attrs, top_level_bracket_attrs};

use crate::resolver::{DefId, DefKind, ResolvedProgram};

use super::parse::lang_item_from_attrs;
use super::registry::{LangItemKind, LangItemMarker, LangItemRegistry};
use super::validate::{auto_variants_for_enum, is_known_lang_item};

/// Span recorded for a registered language item (for duplicate diagnostics).
#[derive(Debug, Clone, Copy)]
struct MarkerSite {
    def_id: DefId,
    span: Span,
}

/// Builds the language item registry for `resolved`, reporting errors into `bag`.
///
/// Scans all modules for `#[lang_item]` attributes on std items, validates each marker against
/// the closed v1 registry, deduplicates by `(kind, name)`, and merges dependency import markers
/// from [`ResolvedProgram::import_lang_items`](crate::resolver::ResolvedProgram) when the
/// current program does not already define the same key. After insertion, variant ctors for
/// registered `Option`/`Result` enums are auto-linked via
/// [`super::validate::auto_variants_for_enum`] when not explicitly marked.
///
/// Diagnostics ([`TypeCheckError::LangItemReserved`](crate::typeck::TypeCheckError::LangItemReserved),
/// [`TypeCheckError::LangItemInvalid`](crate::typeck::TypeCheckError::LangItemInvalid),
/// [`TypeCheckError::LangItemDuplicate`](crate::typeck::TypeCheckError::LangItemDuplicate)) are
/// pushed into `bag` and collection continues — the returned registry contains every
/// successfully registered item even when errors were reported.
///
/// # Panics
///
/// Never panics on malformed resolved programs.
#[must_use]
pub fn build_lang_item_registry(
    resolved: &ResolvedProgram,
    bag: &mut TypeCheckBag,
) -> LangItemRegistry {
    let interner = &resolved.interner;
    let mut sites: std::collections::HashMap<(LangItemKind, String), MarkerSite> =
        std::collections::HashMap::new();

    for module in &resolved.modules {
        let is_std = module.logical_path.starts_with("std::");
        for item in &module.program.items {
            collect_item_marker(resolved, interner, module.id, is_std, item, &mut sites, bag);
        }
    }

    for (&def_id, marker) in &resolved.import_lang_items {
        let key = (marker.kind, marker.name.clone());
        if sites.contains_key(&key) {
            continue;
        }
        if let Some(def) = resolved.defs.get(def_id.index() as usize) {
            sites.insert(
                key,
                MarkerSite {
                    def_id,
                    span: def.span,
                },
            );
        }
    }

    let mut registry = LangItemRegistry::default();
    for ((kind, name), site) in sites {
        let marker = LangItemMarker { name, kind };
        registry.insert_entry(&marker, site.def_id);
    }
    collect_auto_variants(resolved, interner, &mut registry);
    registry
}

/// Scans one top-level item for a `#[lang_item]` attribute and registers it in `sites`.
fn collect_item_marker(
    resolved: &ResolvedProgram,
    interner: &Interner,
    module: u32,
    is_std: bool,
    item: &Node<TopLevelItem>,
    sites: &mut std::collections::HashMap<(LangItemKind, String), MarkerSite>,
    bag: &mut TypeCheckBag,
) {
    let attr_refs = top_level_bracket_attrs(&item.inner);
    let owned_attrs: Vec<Node<phx_syntax::ast::Attribute>> =
        attr_refs.iter().map(|n| (*n).clone()).collect();
    let mut marker = lang_item_from_attrs(interner, &owned_attrs);
    if marker.is_none()
        && let TopLevelDecl::Function(f) = &item.inner.decl
    {
        let fn_attrs = function_bracket_attrs(f);
        let owned_fn: Vec<Node<phx_syntax::ast::Attribute>> =
            fn_attrs.iter().map(|n| (*n).clone()).collect();
        marker = lang_item_from_attrs(interner, &owned_fn);
    }
    let Some(marker) = marker else {
        return;
    };
    if !is_std {
        bag.push(module, TypeCheckError::LangItemReserved { span: item.span });
        return;
    }
    if !is_known_lang_item(marker.kind, &marker.name) {
        bag.push(
            module,
            TypeCheckError::LangItemInvalid {
                detail: format!(
                    "unknown language item `{}` with kind `{}`",
                    marker.name,
                    marker.kind.as_str()
                ),
                span: item.span,
            },
        );
        return;
    }
    let Some(def_id) = def_id_for_item(resolved, module, &item.inner) else {
        bag.push(
            module,
            TypeCheckError::LangItemInvalid {
                detail: format!(
                    "could not resolve definition for language item `{}`",
                    marker.name
                ),
                span: item.span,
            },
        );
        return;
    };
    let key = (marker.kind, marker.name.clone());
    if let Some(prev) = sites.get(&key) {
        if prev.def_id != def_id {
            bag.push(
                module,
                TypeCheckError::LangItemDuplicate {
                    kind: marker.kind.as_str().to_owned(),
                    name: marker.name.clone(),
                    span: item.span,
                    previous_span: prev.span,
                },
            );
        }
        return;
    }
    sites.insert(
        key,
        MarkerSite {
            def_id,
            span: item.span,
        },
    );
}

/// Links variant ctors for registered `Option`/`Result` enums when not explicitly marked.
fn collect_auto_variants(
    resolved: &ResolvedProgram,
    interner: &Interner,
    registry: &mut LangItemRegistry,
) {
    for (enum_name, enum_def) in [
        ("Option", registry.option_enum),
        ("Result", registry.result_enum),
    ] {
        let Some(enum_def) = enum_def else {
            continue;
        };
        let Some(enum_def_record) = resolved.defs.get(enum_def.index() as usize) else {
            continue;
        };
        let module = enum_def_record.module;
        for variant_name in auto_variants_for_enum(enum_name) {
            if registry.variant_registered(variant_name) {
                continue;
            }
            if let Some(variant_def) = find_def(
                resolved,
                interner,
                module,
                variant_name,
                DefKind::EnumVariant,
            ) {
                let marker = LangItemMarker {
                    name: (*variant_name).to_owned(),
                    kind: LangItemKind::Variant,
                };
                registry.insert_entry(&marker, variant_def);
            }
        }
    }
}

/// Resolves the [`DefId`] for a top-level item carrying a language item marker.
fn def_id_for_item(resolved: &ResolvedProgram, module: u32, item: &TopLevelItem) -> Option<DefId> {
    let (name, kind) = match &item.decl {
        TopLevelDecl::Function(f) => (f.name.symbol, DefKind::Fn),
        TopLevelDecl::Enum { name, .. } => (name.symbol, DefKind::Enum),
        TopLevelDecl::Trait { name, .. } => (name.symbol, DefKind::Trait),
        _ => return None,
    };
    let name_str = resolved.interner.resolve(name)?;
    find_def(resolved, &resolved.interner, module, name_str, kind)
}

/// Linear search for a definition by name and kind within `module`.
fn find_def(
    resolved: &ResolvedProgram,
    interner: &Interner,
    module: u32,
    name: &str,
    kind: DefKind,
) -> Option<DefId> {
    for (i, def) in resolved.defs.iter().enumerate() {
        if def.module == module && def.kind == kind && interner.resolves_to(def.name, name) {
            return Some(DefId::from_raw(u32::try_from(i).ok()?));
        }
    }
    None
}

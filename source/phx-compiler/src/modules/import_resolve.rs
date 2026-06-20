//! Shared `#import` binding resolution for file- and block-scoped imports.
//!
//! ## Pass role
//!
//! Called from [`super::resolve_loaded_program`] (and block-scoped import sites in
//! [`crate::resolver::walk`]) while building import prefaces for each module. Resolves a single
//! [`ImportDirective`] to local bindings in value and type namespaces, including `.pxi` type
//! tables for cross-package imports and [`super::discover::is_module_importable`] visibility
//! checks for submodule paths.
//!
//! ## Resolution flow
//!
//! 1. Canonicalize the import target path against workspace and dependency package names.
//! 2. Look up the dependency module in `path_index`; emit nothing when the module was not loaded.
//! 3. Reject private submodule paths via the submodule registry.
//! 4. Build the export map from AST exports or, when a fresh `.pxi` exists, filtered PXI exports.
//! 5. For glob imports, bind every exported symbol; otherwise bind listed identifiers or the
//!    single-item import form.
//! 6. Attach PXI type and lang-item metadata for cross-package defs when build artifacts are fresh.
//!
//! Diagnostics for duplicate imports, missing exports, and private symbols are pushed into
//! [`ImportResolveCtx::bag`]; the function returns successfully collected bindings either way.
//!
//! [`ImportResolveCtx`] bundles the loaded program tables needed for one resolution pass.

use std::collections::{HashMap, HashSet};

use phx_diagnostics::{DiagnosticBag, ResolveError, Span};
use phx_syntax::Interner;
use phx_syntax::Symbol;
use phx_syntax::ast::decl::{ImportDirective, ImportItem};

use crate::lang_items::{LangItemKind, LangItemMarker};
use crate::project::BuildLayout;
use crate::pxi::{PxiFile, PxiType};
use crate::resolver::{Def, DefId, DefKind};

use super::discover::SubmoduleRegistry;
use super::discover::is_module_importable;
use super::loader::{LoadedModule, ModuleId};
use super::path::ModulePath;

type ExportMap = HashMap<Symbol, DefId>;

/// Context for resolving one `#import` directive in a loaded program.
///
/// Holds read-only views of the loaded module graph and mutable side tables for PXI metadata and
/// diagnostics. Constructed per import site in [`super::resolve_loaded_program`] or resolver walk.
pub(crate) struct ImportResolveCtx<'a> {
    /// Module that contains the `#import` being resolved.
    pub module: &'a LoadedModule,
    /// All modules in the loaded program, indexed by [`ModuleId`].
    pub modules: &'a [LoadedModule],
    /// Canonical logical path string → module id (from [`super::loader::LoadedProgram::path_index`]).
    pub path_index: &'a HashMap<String, ModuleId>,
    /// Per-module export maps: symbol → defining [`DefId`] in the dependency module.
    pub exports: &'a [ExportMap],
    /// Flat definition table shared across modules.
    pub defs: &'a [Def],
    /// Build layout when resolving under a project; `None` for standalone loads without artifacts.
    pub layout: Option<&'a BuildLayout>,
    /// Workspace package name used to canonicalize import paths.
    pub workspace_name: &'a str,
    /// Path-dependency package names (first segment aliases for dependency roots).
    pub dep_names: &'a [&'a str],
    /// Interner for resolving symbol names in diagnostics and PXI lookup.
    pub interner: &'a mut Interner,
    /// Structured types from dependency `.pxi` files, keyed by imported [`DefId`].
    pub import_types: &'a mut HashMap<DefId, PxiType>,
    /// Language item markers from dependency `.pxi` files, keyed by imported [`DefId`].
    pub import_lang_items: &'a mut HashMap<DefId, LangItemMarker>,
    /// Diagnostic bag for import errors (duplicate, not found, not exported, private submodule).
    pub bag: &'a mut DiagnosticBag,
    /// Submodule graph for `pub mod` / visibility checks between importer and target.
    pub submodules: &'a SubmoduleRegistry,
}

/// Resolves one `#import` directive into scope bindings.
///
/// Returns `(local_symbol, def_id, is_type_namespace, import_span)` tuples ready to insert into
/// the importer's import preface. When the target module is missing from `path_index`, returns an
/// empty vector without emitting a diagnostic (the loader already reported unloadable modules).
///
/// Duplicate bindings in `seen` produce [`ResolveError::DuplicateImport`]. Missing or non-exported
/// symbols produce [`ResolveError::ImportNotFound`] or [`ResolveError::ImportNotExported`].
/// Imports of non-importable submodule paths produce [`ResolveError::PrivateSubmodule`].
///
/// # Panics
///
/// Never panics on malformed user input; internal index conversions use fallbacks for corrupt
/// module ids.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn resolve_import_directive(
    imp: &ImportDirective,
    span: Span,
    ctx: &mut ImportResolveCtx<'_>,
    seen: &mut HashSet<Symbol>,
) -> Vec<(Symbol, DefId, bool, Span)> {
    let mut bindings = Vec::new();
    let target_path = if imp.items.is_some() {
        ModulePath::from_ast_path(&imp.path, ctx.interner)
    } else {
        ModulePath::split_import_target(&imp.path, ctx.interner).0
    };
    let canonical =
        ModulePath::canonicalize_import(&target_path, ctx.workspace_name, ctx.dep_names);
    let key = canonical.display();
    let Some(&dep_id) = ctx.path_index.get(&key) else {
        return bindings;
    };
    let importer_key = ctx.module.logical_path.display();
    if !is_module_importable(&importer_key, &key, ctx.submodules) {
        ctx.bag.push(
            ctx.module.id.index(),
            ResolveError::PrivateSubmodule {
                span,
                path: key.clone(),
            },
        );
        return bindings;
    }
    let dep_idx = dep_id.index() as usize;
    let dep_exports = exports_for_dependency(
        &key,
        &ctx.exports[dep_idx],
        &ctx.modules[dep_idx],
        ctx.layout,
        ctx.workspace_name,
        ctx.dep_names,
        ctx.interner,
    );

    let glob = imp
        .items
        .as_ref()
        .is_some_and(|list| list.items.iter().any(|i| matches!(i, ImportItem::Glob)));
    if glob {
        for (&sym, &def_id) in &dep_exports {
            if !seen.insert(sym) {
                ctx.bag.push(
                    ctx.module.id.index(),
                    ResolveError::DuplicateImport {
                        span,
                        name: symbol_name(ctx.interner, sym),
                    },
                );
                continue;
            }
            let is_type = is_type_def(ctx.defs, def_id);
            attach_pxi_type(ctx, &key, dep_idx, def_id, sym);
            attach_pxi_lang_item(ctx, &key, dep_idx, def_id, sym);
            bindings.push((sym, def_id, is_type, span));
        }
        return bindings;
    }

    let import_symbols: Vec<Symbol> = if let Some(ref list) = imp.items {
        list.items
            .iter()
            .filter_map(|item| {
                if let ImportItem::Ident(id) = item {
                    Some(id.symbol)
                } else {
                    None
                }
            })
            .collect()
    } else {
        let (_, item) = ModulePath::split_import_target(&imp.path, ctx.interner);
        match ctx.interner.intern(&item) {
            Ok(sym) => vec![sym],
            Err(_) => Vec::new(),
        }
    };

    for sym in import_symbols {
        if let Some(&def_id) = dep_exports.get(&sym) {
            if !seen.insert(sym) {
                ctx.bag.push(
                    ctx.module.id.index(),
                    ResolveError::DuplicateImport {
                        span,
                        name: symbol_name(ctx.interner, sym),
                    },
                );
                continue;
            }
            let is_type = is_type_def(ctx.defs, def_id);
            attach_pxi_type(ctx, &key, dep_idx, def_id, sym);
            attach_pxi_lang_item(ctx, &key, dep_idx, def_id, sym);
            bindings.push((sym, def_id, is_type, span));
        } else if find_private_in_module(ctx.defs, u32::try_from(dep_idx).unwrap_or(u32::MAX), sym)
            .is_some()
        {
            ctx.bag.push(
                ctx.module.id.index(),
                ResolveError::ImportNotExported {
                    span,
                    name: symbol_name(ctx.interner, sym),
                },
            );
        } else {
            ctx.bag.push(
                ctx.module.id.index(),
                ResolveError::ImportNotFound {
                    span,
                    name: symbol_name(ctx.interner, sym),
                    module: key.clone(),
                },
            );
        }
    }
    bindings
}

fn attach_pxi_type(
    ctx: &mut ImportResolveCtx<'_>,
    logical_path: &str,
    dep_idx: usize,
    def_id: DefId,
    sym: Symbol,
) {
    let Some(ty) = pxi_type_for_export(
        ctx.layout,
        logical_path,
        &ctx.modules[dep_idx],
        ctx.workspace_name,
        ctx.dep_names,
        ctx.interner,
        sym,
    ) else {
        return;
    };
    ctx.import_types.insert(def_id, ty);
}

fn attach_pxi_lang_item(
    ctx: &mut ImportResolveCtx<'_>,
    logical_path: &str,
    dep_idx: usize,
    def_id: DefId,
    sym: Symbol,
) {
    let Some(marker) = pxi_lang_item_for_export(
        ctx.layout,
        logical_path,
        &ctx.modules[dep_idx],
        ctx.workspace_name,
        ctx.dep_names,
        ctx.interner,
        sym,
    ) else {
        return;
    };
    ctx.import_lang_items.insert(def_id, marker);
}

fn pxi_lang_item_for_export(
    layout: Option<&BuildLayout>,
    logical_path: &str,
    dep_module: &LoadedModule,
    workspace_package: &str,
    dep_names: &[&str],
    interner: &Interner,
    sym: Symbol,
) -> Option<LangItemMarker> {
    let layout = layout?;
    let pxi_path = layout
        .module_artifacts_resolved(logical_path, workspace_package, dep_names)
        .pxi;
    let pxi = PxiFile::read_from_path(&pxi_path).ok()?;
    if !pxi.source_is_fresh(&dep_module.filesystem) {
        return None;
    }
    let name = interner.resolve(sym).unwrap_or("<?>");
    let exp = pxi.exports.iter().find(|e| e.name == name)?;
    let li = exp.lang_item.as_ref()?;
    let kind = LangItemKind::parse(&li.kind)?;
    Some(LangItemMarker {
        name: li.name.clone(),
        kind,
    })
}

fn pxi_type_for_export(
    layout: Option<&BuildLayout>,
    logical_path: &str,
    dep_module: &LoadedModule,
    workspace_package: &str,
    dep_names: &[&str],
    interner: &Interner,
    sym: Symbol,
) -> Option<PxiType> {
    let layout = layout?;
    let pxi_path = layout
        .module_artifacts_resolved(logical_path, workspace_package, dep_names)
        .pxi;
    let pxi = PxiFile::read_from_path(&pxi_path).ok()?;
    if !pxi.source_is_fresh(&dep_module.filesystem) {
        return None;
    }
    let name = interner.resolve(sym).unwrap_or("<?>");
    pxi.exports
        .iter()
        .find(|e| e.name == name)
        .and_then(|e| e.ty.clone())
}

fn symbol_name(interner: &Interner, sym: Symbol) -> String {
    interner.resolve_display(sym)
}

fn exports_for_dependency(
    logical_path: &str,
    ast_exports: &ExportMap,
    dep_module: &LoadedModule,
    layout: Option<&BuildLayout>,
    workspace_package: &str,
    dep_names: &[&str],
    interner: &Interner,
) -> ExportMap {
    let Some(layout) = layout else {
        return ast_exports.clone();
    };
    let pxi_path = layout
        .module_artifacts_resolved(logical_path, workspace_package, dep_names)
        .pxi;
    if !pxi_path.is_file() {
        return ast_exports.clone();
    }
    let Ok(pxi) = PxiFile::read_from_path(&pxi_path) else {
        return ast_exports.clone();
    };
    if !pxi.source_is_fresh(&dep_module.filesystem) {
        return ast_exports.clone();
    }
    let mut from_pxi = ExportMap::new();
    for exp in &pxi.exports {
        for (&sym, &def_id) in ast_exports {
            if interner.resolves_to(sym, &exp.name) {
                from_pxi.insert(sym, def_id);
            }
        }
    }
    if from_pxi.is_empty() {
        ast_exports.clone()
    } else {
        from_pxi
    }
}

fn is_type_def(defs: &[Def], def_id: DefId) -> bool {
    defs.get(def_id.index() as usize).is_some_and(|d| {
        matches!(
            d.kind,
            DefKind::Struct
                | DefKind::Enum
                | DefKind::TypeAlias
                | DefKind::Trait
                | DefKind::GenericParam
        )
    })
}

fn find_private_in_module(defs: &[Def], module: u32, sym: Symbol) -> Option<DefId> {
    defs.iter()
        .enumerate()
        .find(|(_, d)| d.module == module && d.name == sym)
        .map(|(i, _)| DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)))
}

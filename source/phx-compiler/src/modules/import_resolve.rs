//! Shared `#import` binding resolution for file- and block-scoped imports.

use std::collections::{HashMap, HashSet};

use phx_diagnostics::{DiagnosticBag, ResolveError, Span};
use phx_syntax::Interner;
use phx_syntax::Symbol;
use phx_syntax::ast::decl::{ImportDirective, ImportItem};

use crate::project::BuildLayout;
use crate::pxi::{PxiFile, PxiType};
use crate::resolver::{Def, DefId, DefKind};

use super::loader::{LoadedModule, ModuleId};
use super::path::ModulePath;

type ExportMap = HashMap<Symbol, DefId>;

/// Context for resolving one `#import` directive in a loaded program.
pub(crate) struct ImportResolveCtx<'a> {
    /// Module containing the import.
    pub module: &'a LoadedModule,
    /// All modules in the loaded program.
    pub modules: &'a [LoadedModule],
    /// Logical path → module id.
    pub path_index: &'a HashMap<String, ModuleId>,
    /// Per-module export maps.
    pub exports: &'a [ExportMap],
    /// All definitions.
    pub defs: &'a [Def],
    /// Build layout when resolving under a project.
    pub layout: Option<&'a BuildLayout>,
    /// Workspace package name.
    pub workspace_name: &'a str,
    /// Path-dependency package names.
    pub dep_names: &'a [&'a str],
    /// Interner for symbol names.
    pub interner: &'a mut Interner,
    /// Structured types from dependency `.pxi` for imported defs.
    pub import_types: &'a mut HashMap<DefId, PxiType>,
    /// Diagnostic bag.
    pub bag: &'a mut DiagnosticBag,
}

/// Resolves one `#import` into scope bindings `(symbol, def_id, is_type, span)`.
#[allow(clippy::too_many_arguments)]
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
                        name: ctx.interner.resolve(sym).to_owned(),
                    },
                );
                continue;
            }
            let is_type = is_type_def(ctx.defs, def_id);
            attach_pxi_type(ctx, &key, dep_idx, def_id, sym);
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
                        name: ctx.interner.resolve(sym).to_owned(),
                    },
                );
                continue;
            }
            let is_type = is_type_def(ctx.defs, def_id);
            attach_pxi_type(ctx, &key, dep_idx, def_id, sym);
            bindings.push((sym, def_id, is_type, span));
        } else if find_private_in_module(ctx.defs, u32::try_from(dep_idx).unwrap_or(u32::MAX), sym)
            .is_some()
        {
            ctx.bag.push(
                ctx.module.id.index(),
                ResolveError::ImportNotExported {
                    span,
                    name: ctx.interner.resolve(sym).to_owned(),
                },
            );
        } else {
            ctx.bag.push(
                ctx.module.id.index(),
                ResolveError::ImportNotFound {
                    span,
                    name: ctx.interner.resolve(sym).to_owned(),
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
    let name = interner.resolve(sym);
    pxi.exports
        .iter()
        .find(|e| e.name == name)
        .and_then(|e| e.ty.clone())
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
            if interner.resolve(sym) == exp.name {
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

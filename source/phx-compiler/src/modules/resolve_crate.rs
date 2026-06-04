//! Cross-module name resolution for a loaded crate.

use std::collections::{HashMap, HashSet};

use phx_diagnostics::{DiagnosticBag, ResolveError, Span};
use phx_syntax::Interner;
use phx_syntax::ast::decl::{ImportItem, Program, TopLevelDecl};
use phx_syntax::{SourceFile, Symbol};

use crate::resolver::scopes::ScopeStack;
use crate::resolver::{Def, DefId, DefKind, ResolvedProgram, Resolver, SourceModule};

use super::loader::{LoadedCrate, LoadedModule, ModuleId};
use super::path::ModulePath;
use crate::project::{BuildLayout, PackageType};
use crate::pxi::PxiFile;

type ExportMap = HashMap<Symbol, DefId>;

/// Resolves all modules in `loaded` into a [`ResolvedProgram`].
///
/// # Errors
///
/// Returns [`DiagnosticBag`] when imports, duplicates, or `main` validation fail.
#[allow(clippy::too_many_lines)]
pub fn resolve_crate(loaded: LoadedCrate) -> Result<ResolvedProgram, DiagnosticBag> {
    let LoadedCrate {
        mut interner,
        modules,
        root,
        path_index,
        package_type,
        package_name,
        dep_package_names,
        build_layout,
    } = loaded;
    let layout = build_layout.as_ref();
    let dep_name_refs: Vec<&str> = dep_package_names.iter().map(String::as_str).collect();
    let mut bag = DiagnosticBag::new();
    let mut defs = Vec::new();
    let mut exports: Vec<ExportMap> = vec![HashMap::new(); modules.len()];
    let mut main_fn = None;
    let mut phase1_skip: HashSet<u32> = HashSet::new();

    let source_modules: Vec<SourceModule> = modules
        .iter()
        .map(|m| SourceModule {
            id: m.id.index(),
            logical_path: m.logical_path.display(),
            filesystem: m.filesystem.clone(),
            source: m.source.clone(),
            program: m.program.clone(),
        })
        .collect();

    // Phase 1: collect definitions and exports.
    for (idx, module) in modules.iter().enumerate() {
        let sf = SourceFile::new(module.program.clone(), interner.clone());
        let mut resolver = Resolver {
            source: &sf,
            defs: Vec::new(),
            scopes: ScopeStack::default(),
            bag: DiagnosticBag::new(),
            resolutions: HashMap::new(),
            main_fn: None,
            current_module: module.id.index(),
            root_module: root.index(),
            logical_path: &source_modules[idx].logical_path,
            allow_imports: true,
            collect_only: true,
            import_bindings: Vec::new(),
        };
        resolver.resolve_program();
        if resolver.main_fn.is_some() {
            if package_type == PackageType::Lib {
                let span = main_function_span(&module.program, &interner)
                    .unwrap_or_else(|| Span::new(0, 1));
                bag.push(
                    module.id.index(),
                    ResolveError::MainForbiddenInLib {
                        span,
                        module: source_modules[idx].logical_path.clone(),
                    },
                );
            } else if module.id == root {
                main_fn = resolver.main_fn;
            }
        }
        if module.id == root && package_type == PackageType::Bin {
            resolver.check_main();
        }
        if resolver.bag.has_errors() {
            phase1_skip.insert(module.id.index());
        }
        for e in resolver.bag.into_errors() {
            bag.push_located(e);
        }
        for def in resolver.defs {
            let id = DefId::from_raw(u32::try_from(defs.len()).unwrap_or(u32::MAX));
            if def.exported {
                exports[idx].insert(def.name, id);
            }
            defs.push(def);
        }
    }

    // Phase 2: resolve bodies with import prefaces (skip modules that failed phase 1).
    let mut resolutions = HashMap::new();
    for (idx, module) in modules.iter().enumerate() {
        if phase1_skip.contains(&module.id.index()) {
            continue;
        }
        let bindings = build_import_bindings(
            module,
            &modules,
            &path_index,
            &exports,
            &defs,
            layout,
            &mut interner,
            &package_name,
            &dep_name_refs,
            &mut bag,
        );
        let sf = SourceFile::new(module.program.clone(), interner.clone());
        let mut resolver = Resolver {
            source: &sf,
            defs: defs.clone(),
            scopes: ScopeStack::default(),
            bag: DiagnosticBag::new(),
            resolutions: HashMap::new(),
            main_fn: None,
            current_module: module.id.index(),
            root_module: root.index(),
            logical_path: &source_modules[idx].logical_path,
            allow_imports: true,
            collect_only: false,
            import_bindings: bindings,
        };
        resolver.resolve_program();
        for e in resolver.bag.into_errors() {
            bag.push_located(e);
        }
        resolutions.extend(resolver.resolutions);
    }

    if package_type == PackageType::Bin && main_fn.is_none() {
        let span = root_hint_span(&source_modules, root.index());
        bag.push(root.index(), ResolveError::MissingMain { span });
    }

    if bag.has_errors() {
        return Err(bag);
    }

    let root_index = root.index();
    let program = if let Some(m) = source_modules.iter().find(|m| m.id == root_index) {
        m.program.clone()
    } else {
        source_modules.first().map_or_else(
            || phx_syntax::ast::decl::Program {
                imports: Vec::new(),
                items: Vec::new(),
            },
            |m| m.program.clone(),
        )
    };

    Ok(ResolvedProgram {
        program,
        modules: source_modules,
        root: root_index,
        interner,
        defs,
        resolutions,
        main_fn,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_import_bindings(
    module: &LoadedModule,
    modules: &[LoadedModule],
    path_index: &HashMap<String, ModuleId>,
    exports: &[ExportMap],
    defs: &[Def],
    layout: Option<&BuildLayout>,
    interner: &mut Interner,
    workspace_name: &str,
    dep_names: &[&str],
    bag: &mut DiagnosticBag,
) -> Vec<(Symbol, DefId, bool, Span)> {
    let mut bindings = Vec::new();
    let mut seen: HashSet<Symbol> = HashSet::new();

    for imp in &module.program.imports {
        let target_path = if imp.inner.items.is_some() {
            ModulePath::from_ast_path(&imp.inner.path, interner)
        } else {
            ModulePath::split_import_target(&imp.inner.path, interner).0
        };
        let canonical = ModulePath::canonicalize_import(&target_path, workspace_name, dep_names);
        let key = canonical.display();
        let Some(&dep_id) = path_index.get(&key) else {
            continue;
        };
        let dep_idx = dep_id.index() as usize;
        let dep_exports =
            exports_for_dependency(&key, &exports[dep_idx], &modules[dep_idx], layout, interner);

        let glob = imp
            .inner
            .items
            .as_ref()
            .is_some_and(|list| list.items.iter().any(|i| matches!(i, ImportItem::Glob)));
        if glob {
            for (&sym, &def_id) in &dep_exports {
                if !seen.insert(sym) {
                    bag.push(
                        module.id.index(),
                        ResolveError::DuplicateImport {
                            span: imp.span,
                            name: interner.resolve(sym).to_owned(),
                        },
                    );
                    continue;
                }
                let is_type = is_type_def(defs, def_id);
                bindings.push((sym, def_id, is_type, imp.span));
            }
            continue;
        }

        let import_symbols: Vec<Symbol> = if let Some(ref list) = imp.inner.items {
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
            let (_, item) = ModulePath::split_import_target(&imp.inner.path, interner);
            match interner.intern(&item) {
                Ok(sym) => vec![sym],
                Err(_) => Vec::new(),
            }
        };

        for sym in import_symbols {
            if let Some(&def_id) = dep_exports.get(&sym) {
                if !seen.insert(sym) {
                    bag.push(
                        module.id.index(),
                        ResolveError::DuplicateImport {
                            span: imp.span,
                            name: interner.resolve(sym).to_owned(),
                        },
                    );
                    continue;
                }
                let is_type = is_type_def(defs, def_id);
                bindings.push((sym, def_id, is_type, imp.span));
            } else if find_private_in_module(defs, u32::try_from(dep_idx).unwrap_or(u32::MAX), sym)
                .is_some()
            {
                bag.push(
                    module.id.index(),
                    ResolveError::ImportNotExported {
                        span: imp.span,
                        name: interner.resolve(sym).to_owned(),
                    },
                );
            } else {
                bag.push(
                    module.id.index(),
                    ResolveError::ImportNotFound {
                        span: imp.span,
                        name: interner.resolve(sym).to_owned(),
                        module: key.clone(),
                    },
                );
            }
        }
    }
    bindings
}

/// Export map for an import target: when `.pxi` is fresh, restrict to symbols listed in the interface.
fn exports_for_dependency(
    logical_path: &str,
    ast_exports: &ExportMap,
    dep_module: &LoadedModule,
    layout: Option<&BuildLayout>,
    interner: &Interner,
) -> ExportMap {
    let Some(layout) = layout else {
        return ast_exports.clone();
    };
    let pxi_path = layout.module_artifacts(logical_path).pxi;
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

fn main_function_span(program: &Program, interner: &Interner) -> Option<Span> {
    for item in &program.items {
        if let TopLevelDecl::Function(f) = &item.inner.decl
            && interner.resolve(f.name.symbol) == "main"
        {
            return Some(f.name.span);
        }
    }
    None
}

fn root_hint_span(modules: &[SourceModule], root: u32) -> Span {
    let Some(m) = modules.iter().find(|m| m.id == root) else {
        return Span::new(0, 1);
    };
    if let Some(item) = m.program.items.first() {
        item.span
    } else if let Some(imp) = m.program.imports.first() {
        imp.span
    } else {
        Span::new(0, 1)
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

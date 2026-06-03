//! Cross-module name resolution for a loaded crate.

use std::collections::HashMap;

use phx_diagnostics::{DiagnosticBag, ResolveError};
use phx_syntax::Interner;
use phx_syntax::ast::decl::ImportItem;
use phx_syntax::{SourceFile, Symbol};

use crate::resolver::scopes::ScopeStack;
use crate::resolver::{Def, DefId, DefKind, ResolvedProgram, Resolver, SourceModule};

use super::loader::{LoadedCrate, LoadedModule, ModuleId};
use super::path::ModulePath;
use crate::project::PackageType;

type ExportMap = HashMap<Symbol, DefId>;

/// Resolves all modules in `loaded` into a [`ResolvedProgram`].
pub fn resolve_crate(loaded: LoadedCrate) -> Result<ResolvedProgram, DiagnosticBag> {
    let LoadedCrate {
        mut interner,
        modules,
        root,
        path_index,
        package_type,
        package_name,
        dep_package_names,
        ..
    } = loaded;
    let dep_name_refs: Vec<&str> = dep_package_names.iter().map(String::as_str).collect();
    let mut bag = DiagnosticBag::new();
    let mut defs = Vec::new();
    let mut exports: Vec<ExportMap> = vec![HashMap::new(); modules.len()];
    let mut main_fn = None;

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
                bag.push(ResolveError::MainForbiddenInLib {
                    span: phx_diagnostics::Span::new(0, 0),
                    module: source_modules[idx].logical_path.clone(),
                });
            } else if module.id == root {
                main_fn = resolver.main_fn;
            }
        }
        if module.id == root && package_type == PackageType::Bin {
            resolver.check_main();
        }
        for e in resolver.bag.into_errors() {
            bag.push(e);
        }
        for def in resolver.defs {
            let id = DefId::from_raw(u32::try_from(defs.len()).unwrap_or(u32::MAX));
            if def.exported {
                exports[idx].insert(def.name, id);
            }
            defs.push(def);
        }
    }

    if bag.has_errors() {
        return Err(bag);
    }

    // Phase 2: resolve bodies with import prefaces.
    let mut resolutions = HashMap::new();
    for (idx, module) in modules.iter().enumerate() {
        let bindings = build_import_bindings(
            module,
            &path_index,
            &exports,
            &defs,
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
            bag.push(e);
        }
        resolutions.extend(resolver.resolutions);
    }

    if package_type == PackageType::Bin && main_fn.is_none() {
        bag.push(ResolveError::MissingMain);
    }

    if bag.has_errors() {
        return Err(bag);
    }

    let root_module = source_modules
        .iter()
        .find(|m| m.id == root.index())
        .expect("root module");

    Ok(ResolvedProgram {
        program: root_module.program.clone(),
        modules: source_modules,
        root: root.index(),
        interner,
        defs,
        resolutions,
        main_fn,
    })
}

fn build_import_bindings(
    module: &LoadedModule,
    path_index: &HashMap<String, ModuleId>,
    exports: &[ExportMap],
    defs: &[Def],
    interner: &mut Interner,
    workspace_name: &str,
    dep_names: &[&str],
    bag: &mut DiagnosticBag,
) -> Vec<(Symbol, DefId, bool)> {
    let mut bindings = Vec::new();
    let mut seen: HashMap<Symbol, ()> = HashMap::new();

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
        let dep_exports = &exports[dep_idx];

        let glob = imp
            .inner
            .items
            .as_ref()
            .is_some_and(|list| list.items.iter().any(|i| matches!(i, ImportItem::Glob)));
        if glob {
            for (&sym, &def_id) in dep_exports {
                if seen.insert(sym, ()).is_some() {
                    bag.push(ResolveError::DuplicateImport {
                        span: imp.span,
                        name: interner.resolve(sym).to_owned(),
                    });
                    continue;
                }
                let is_type = is_type_def(defs, def_id);
                bindings.push((sym, def_id, is_type));
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
            vec![interner.intern(&item)]
        };

        for sym in import_symbols {
            if let Some(&def_id) = dep_exports.get(&sym) {
                if seen.insert(sym, ()).is_some() {
                    bag.push(ResolveError::DuplicateImport {
                        span: imp.span,
                        name: interner.resolve(sym).to_owned(),
                    });
                    continue;
                }
                let is_type = is_type_def(defs, def_id);
                bindings.push((sym, def_id, is_type));
            } else if find_private_in_module(defs, dep_idx as u32, sym).is_some() {
                bag.push(ResolveError::ImportNotExported {
                    span: imp.span,
                    name: interner.resolve(sym).to_owned(),
                });
            } else {
                bag.push(ResolveError::ImportNotFound {
                    span: imp.span,
                    name: interner.resolve(sym).to_owned(),
                    module: key.clone(),
                });
            }
        }
    }
    bindings
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

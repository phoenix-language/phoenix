//! Cross-module name resolution for a loaded program.

use std::collections::{HashMap, HashSet};

use phx_diagnostics::{DiagnosticBag, ResolveError, Span};
use phx_syntax::Interner;
use phx_syntax::ast::decl::{Program, TopLevelDecl};
use phx_syntax::{SourceFile, Symbol};

use crate::resolver::scopes::ScopeStack;
use crate::resolver::{
    DefId, DefIdOverflow, ProgramImportEnv, ResolvedProgram, Resolver, SourceModule,
};

use super::discover::SubmoduleRegistry;
use super::import_resolve::{ImportResolveCtx, resolve_import_directive};
use super::loader::{LoadedModule, LoadedProgram};
use super::prelude::{PreludeCtx, prelude_bindings};
use crate::project::PackageType;
use crate::pxi::PxiType;

type ExportMap = HashMap<Symbol, DefId>;

/// Maps a module-local [`DefId`] into the merged program table.
fn try_remap_local_def_id(def_base: usize, local: DefId) -> Result<DefId, DefIdOverflow> {
    let index = def_base
        .checked_add(local.index() as usize)
        .ok_or(DefIdOverflow)?;
    DefId::try_from_index(index)
}

/// Allocates the next global [`DefId`] when appending defs during module merge.
fn try_alloc_merged_def_id(len: usize) -> Result<DefId, DefIdOverflow> {
    DefId::try_from_index(len)
}

/// Resolves all modules in `loaded` into a [`ResolvedProgram`].
///
/// # Errors
///
/// Returns [`DiagnosticBag`] when imports, duplicates, or `main` validation fail.
#[allow(clippy::too_many_lines)]
pub fn resolve_loaded_program(loaded: LoadedProgram) -> Result<ResolvedProgram, DiagnosticBag> {
    let LoadedProgram {
        mut interner,
        modules,
        root,
        path_index,
        package_type,
        package_name,
        dep_package_names,
        build_layout,
        prelude_enabled,
        submodules,
    } = loaded;
    let layout = build_layout.as_ref();
    let dep_name_refs: Vec<&str> = dep_package_names.iter().map(String::as_str).collect();
    let mut bag = DiagnosticBag::new();
    let mut defs = Vec::new();
    let mut def_attrs = crate::attrs::DefAttrs::new();
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
    let mut def_table_full = false;
    for (idx, module) in modules.iter().enumerate() {
        let sf = SourceFile::new(module.program.clone(), interner.clone());
        let mut resolver = Resolver {
            source: &sf,
            defs: Vec::new(),
            scopes: ScopeStack::default(),
            bag: DiagnosticBag::new(),
            resolutions: HashMap::new(),
            closures: HashMap::new(),
            closure_stack: Vec::new(),
            trait_impls: Vec::new(),
            main_fn: None,
            current_module: module.id.index(),
            root_module: root.index(),
            logical_path: &source_modules[idx].logical_path,
            allow_imports: true,
            collect_only: true,
            import_bindings: Vec::new(),
            self_type_depth: 0,
            import_env: None,
            shared_interner: None,
            import_types: None,
            import_lang_items: None,
            def_attrs: crate::attrs::DefAttrs::new(),
            def_table_full: false,
        };
        resolver.resolve_program();
        if resolver.main_fn.is_some() && package_type == PackageType::Lib {
            let span =
                main_function_span(&module.program, &interner).unwrap_or_else(|| Span::new(0, 1));
            bag.push(
                module.id.index(),
                ResolveError::MainForbiddenInLib {
                    span,
                    module: source_modules[idx].logical_path.clone(),
                },
            );
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
        let def_base = defs.len();
        if module.id == root
            && let Some(local_main) = resolver.main_fn
        {
            if let Ok(global) = try_remap_local_def_id(def_base, local_main) {
                main_fn = Some(global);
            } else {
                let span = main_function_span(&module.program, &interner)
                    .unwrap_or_else(|| Span::new(0, 1));
                bag.push(module.id.index(), ResolveError::ProgramTooLarge { span });
                def_table_full = true;
            }
        }
        for def in resolver.defs {
            if def_table_full {
                break;
            }
            let Ok(id) = try_alloc_merged_def_id(defs.len()) else {
                bag.push(
                    module.id.index(),
                    ResolveError::ProgramTooLarge { span: def.span },
                );
                def_table_full = true;
                break;
            };
            if def.exported {
                exports[idx].insert(def.name, id);
            }
            defs.push(def);
        }
        for (local_id, attrs) in resolver.def_attrs {
            if def_table_full {
                break;
            }
            let span = defs
                .get(def_base + local_id.index() as usize)
                .map_or_else(|| Span::new(0, 1), |d| d.span);
            if let Ok(global) = try_remap_local_def_id(def_base, local_id) {
                def_attrs.insert(global, attrs);
            } else {
                bag.push(module.id.index(), ResolveError::ProgramTooLarge { span });
                def_table_full = true;
            }
        }
    }

    apply_reexports(&modules, &submodules, &mut exports, &mut interner, &mut bag);

    // Phase 2: resolve bodies with import prefaces (skip modules that failed phase 1).
    let mut resolutions = HashMap::new();
    let mut closures = HashMap::new();
    let mut import_types: HashMap<DefId, PxiType> = HashMap::new();
    let mut import_lang_items: HashMap<DefId, crate::lang_items::LangItemMarker> = HashMap::new();
    for (idx, module) in modules.iter().enumerate() {
        if phase1_skip.contains(&module.id.index()) {
            continue;
        }
        let import_env = ProgramImportEnv {
            modules: &modules,
            path_index: &path_index,
            exports: &exports,
            defs: &defs,
            layout,
            workspace_name: &package_name,
            dep_names: &dep_name_refs,
            submodules: &submodules,
        };
        let bindings = build_import_bindings(
            module,
            &import_env,
            &mut interner,
            &mut bag,
            &mut import_types,
            &mut import_lang_items,
            prelude_enabled,
            &package_name,
        );
        let sf = SourceFile::new(module.program.clone(), interner.clone());
        let mut resolver = Resolver {
            source: &sf,
            defs: defs.clone(),
            scopes: ScopeStack::default(),
            bag: DiagnosticBag::new(),
            resolutions: HashMap::new(),
            closures: HashMap::new(),
            closure_stack: Vec::new(),
            trait_impls: Vec::new(),
            main_fn: None,
            current_module: module.id.index(),
            root_module: root.index(),
            logical_path: &source_modules[idx].logical_path,
            allow_imports: true,
            collect_only: false,
            import_bindings: bindings,
            self_type_depth: 0,
            import_env: Some(import_env),
            shared_interner: Some(&mut interner),
            import_types: Some(&mut import_types),
            import_lang_items: Some(&mut import_lang_items),
            def_attrs: crate::attrs::DefAttrs::new(),
            def_table_full: false,
        };
        let def_base = defs.len();
        resolver.resolve_program();
        let extra_defs = if resolver.defs.len() > def_base {
            resolver.defs[def_base..].to_vec()
        } else {
            Vec::new()
        };
        for e in resolver.bag.into_errors() {
            bag.push_located(e);
        }
        resolutions.extend(resolver.resolutions);
        closures.extend(resolver.closures);
        let extra_attrs = resolver.def_attrs;
        for def in extra_defs {
            if def_table_full {
                break;
            }
            if try_alloc_merged_def_id(defs.len()).is_ok() {
                defs.push(def);
            } else {
                bag.push(
                    module.id.index(),
                    ResolveError::ProgramTooLarge { span: def.span },
                );
                def_table_full = true;
            }
        }
        for (local_id, attrs) in extra_attrs {
            if def_table_full {
                break;
            }
            let span = defs
                .get(def_base + local_id.index() as usize)
                .map_or_else(|| Span::new(0, 1), |d| d.span);
            if let Ok(global) = try_remap_local_def_id(def_base, local_id) {
                def_attrs.insert(global, attrs);
            } else {
                bag.push(module.id.index(), ResolveError::ProgramTooLarge { span });
                def_table_full = true;
            }
        }
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
        closures,
        main_fn,
        import_types,
        import_lang_items,
        def_attrs,
    })
}

fn apply_reexports(
    modules: &[LoadedModule],
    submodules: &SubmoduleRegistry,
    exports: &mut [ExportMap],
    interner: &mut Interner,
    bag: &mut DiagnosticBag,
) {
    let path_to_idx: HashMap<String, usize> = modules
        .iter()
        .enumerate()
        .map(|(i, m)| (m.logical_path.display(), i))
        .collect();

    for (idx, module) in modules.iter().enumerate() {
        let parent_key = module.logical_path.display();
        let Some(list) = submodules.reexports.get(&parent_key) else {
            continue;
        };
        for re in list {
            let Some(export_name) = re.segments.last() else {
                continue;
            };
            let Ok(export_sym) = interner.intern(export_name) else {
                continue;
            };
            let target_def = if re.segments.len() == 1 {
                exports[idx].get(&export_sym).copied()
            } else if re.segments.len() == 2 {
                let child_key = format!("{}::{}", parent_key, re.segments[0]);
                let Some(child_idx) = path_to_idx.get(&child_key).copied() else {
                    bag.push(
                        module.id.index(),
                        ResolveError::ModuleNotFound {
                            span: re.span,
                            path: child_key,
                        },
                    );
                    continue;
                };
                let Ok(item_sym) = interner.intern(&re.segments[1]) else {
                    continue;
                };
                exports[child_idx].get(&item_sym).copied()
            } else {
                bag.push(
                    module.id.index(),
                    ResolveError::ModuleNotFound {
                        span: re.span,
                        path: re.segments.join("::"),
                    },
                );
                continue;
            };
            if let Some(def_id) = target_def {
                exports[idx].insert(export_sym, def_id);
            } else {
                bag.push(
                    module.id.index(),
                    ResolveError::ImportNotExported {
                        span: re.span,
                        name: export_name.clone(),
                    },
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_import_bindings(
    module: &LoadedModule,
    env: &ProgramImportEnv<'_>,
    interner: &mut Interner,
    bag: &mut DiagnosticBag,
    import_types: &mut HashMap<DefId, PxiType>,
    import_lang_items: &mut HashMap<DefId, crate::lang_items::LangItemMarker>,
    prelude_enabled: bool,
    workspace_name: &str,
) -> Vec<(Symbol, DefId, bool, Span)> {
    let mut bindings = Vec::new();
    let mut seen: HashSet<Symbol> = HashSet::new();

    for imp in &module.program.imports {
        let mut ctx = ImportResolveCtx {
            module,
            modules: env.modules,
            path_index: env.path_index,
            exports: env.exports,
            defs: env.defs,
            layout: env.layout,
            workspace_name: env.workspace_name,
            dep_names: env.dep_names,
            interner,
            import_types,
            import_lang_items,
            bag,
            submodules: env.submodules,
        };
        bindings.extend(resolve_import_directive(
            &imp.inner, imp.span, &mut ctx, &mut seen,
        ));
    }
    if prelude_enabled && module_belongs_to_workspace(&module.logical_path, workspace_name) {
        let prelude_ctx = PreludeCtx {
            path_index: env.path_index,
            exports: env.exports,
            interner,
            span: Span::new(0, 1),
        };
        bindings.extend(prelude_bindings(&prelude_ctx, &seen));
    }
    bindings
}

fn module_belongs_to_workspace(path: &super::path::ModulePath, workspace_name: &str) -> bool {
    let key = path.display();
    key == workspace_name || key.starts_with(&format!("{workspace_name}::"))
}

fn main_function_span(program: &Program, interner: &Interner) -> Option<Span> {
    for item in &program.items {
        if let TopLevelDecl::Function(f) = &item.inner.decl
            && interner.resolves_to(f.name.symbol, "main")
        {
            return Some(f.name.span);
        }
    }
    None
}

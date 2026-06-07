//! Cross-module name resolution for a loaded program.

use std::collections::{HashMap, HashSet};

use phx_diagnostics::{DiagnosticBag, ResolveError, Span};
use phx_syntax::Interner;
use phx_syntax::ast::decl::{Program, TopLevelDecl};
use phx_syntax::{SourceFile, Symbol};

use crate::resolver::scopes::ScopeStack;
use crate::resolver::{DefId, ProgramImportEnv, ResolvedProgram, Resolver, SourceModule};

use super::import_resolve::{ImportResolveCtx, resolve_import_directive};
use super::loader::{LoadedModule, LoadedProgram};
use crate::project::PackageType;
use crate::pxi::PxiType;

type ExportMap = HashMap<Symbol, DefId>;

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
            def_attrs: crate::attrs::DefAttrs::new(),
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
        let def_base = defs.len();
        for def in resolver.defs {
            let id = DefId::from_raw(u32::try_from(defs.len()).unwrap_or(u32::MAX));
            if def.exported {
                exports[idx].insert(def.name, id);
            }
            defs.push(def);
        }
        for (local_id, attrs) in resolver.def_attrs {
            let global = DefId::from_raw(
                u32::try_from(def_base)
                    .ok()
                    .and_then(|b| b.checked_add(local_id.index()))
                    .unwrap_or(u32::MAX),
            );
            def_attrs.insert(global, attrs);
        }
    }

    // Phase 2: resolve bodies with import prefaces (skip modules that failed phase 1).
    let mut resolutions = HashMap::new();
    let mut closures = HashMap::new();
    let mut import_types: HashMap<DefId, PxiType> = HashMap::new();
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
        };
        let bindings = build_import_bindings(
            module,
            &import_env,
            &mut interner,
            &mut bag,
            &mut import_types,
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
            def_attrs: crate::attrs::DefAttrs::new(),
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
        defs.extend(extra_defs);
        for (local_id, attrs) in extra_attrs {
            let global = DefId::from_raw(
                u32::try_from(def_base)
                    .ok()
                    .and_then(|b| b.checked_add(local_id.index()))
                    .unwrap_or(u32::MAX),
            );
            def_attrs.insert(global, attrs);
        }
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
        closures,
        main_fn,
        import_types,
        def_attrs,
    })
}

fn build_import_bindings(
    module: &LoadedModule,
    env: &ProgramImportEnv<'_>,
    interner: &mut Interner,
    bag: &mut DiagnosticBag,
    import_types: &mut HashMap<DefId, PxiType>,
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
            bag,
        };
        bindings.extend(resolve_import_directive(
            &imp.inner, imp.span, &mut ctx, &mut seen,
        ));
    }
    bindings
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

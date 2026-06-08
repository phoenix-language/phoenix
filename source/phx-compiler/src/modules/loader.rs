//! Load all modules reachable from the entry file.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use phx_diagnostics::{DiagnosticBag, ResolveError};
use phx_syntax::{Interner, Program, all_imports, parse_with_interner};

use crate::cfg::{CompileCfg, strip_cfg};
use crate::derive::expand_derives;

use super::SourceText;
use super::discover::{
    SubmoduleRegistry, effective_submodule_parent, resolve_submodule_file,
    validate_ambiguous_module_entries, validate_missing_module_entries, validate_orphan_files,
};
use super::graph::{collect_edges, topo_sort_with_pxi_escape};
use super::load_context::ProgramLoadContext;
use super::path::ModulePath;
use crate::project::{BuildLayout, PackageType};

/// Dense module identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleId(u32);

impl ModuleId {
    /// Creates a module id from a raw index.
    #[must_use]
    pub const fn from_raw(index: u32) -> Self {
        Self(index)
    }

    /// Returns the raw index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// One loaded source module.
#[derive(Debug, Clone)]
pub struct LoadedModule {
    /// Module id.
    pub id: ModuleId,
    /// Logical path (`a::b`).
    pub logical_path: ModulePath,
    /// Absolute path to the `.phx` file.
    pub filesystem: PathBuf,
    /// Source text.
    pub source: SourceText,
    /// Parsed program (imports + items).
    pub program: Program,
}

/// All modules in a loaded program before resolve.
#[derive(Debug, Clone)]
pub struct LoadedProgram {
    /// Shared interner across modules.
    pub interner: Interner,
    /// Modules in topological order.
    pub modules: Vec<LoadedModule>,
    /// Entry (root) module id.
    pub root: ModuleId,
    /// `logical_path.display()` → id.
    pub path_index: HashMap<String, ModuleId>,
    /// Workspace package name.
    pub package_name: String,
    /// Workspace package type.
    pub package_type: PackageType,
    /// Names of path-dependency packages.
    pub dep_package_names: Vec<String>,
    /// Build artifact paths when loaded under a project layout (for `.pxi` import surface).
    pub build_layout: Option<BuildLayout>,
    /// Inject std prelude bindings during resolve.
    pub prelude_enabled: bool,
    /// Submodule declarations discovered while loading.
    pub submodules: SubmoduleRegistry,
}

/// Loads the module graph starting at `entry_file` under `module_root` (single-package fallback).
///
/// # Errors
///
/// Returns I/O or parse errors via `bag`; also records module-not-found and cycles.
pub fn load_program(
    entry_file: &Path,
    module_root: &Path,
    bag: &mut DiagnosticBag,
) -> Option<LoadedProgram> {
    let ctx = ProgramLoadContext {
        workspace: super::load_context::PackageRoot {
            name: infer_package_name(module_root),
            module_src: module_root
                .canonicalize()
                .unwrap_or_else(|_| module_root.to_path_buf()),
            package_type: PackageType::Bin,
        },
        dependencies: Vec::new(),
        prelude: false,
    };
    load_program_with_context(entry_file, &ctx, None, bag)
}

/// Loads a program from `entry_file` using workspace + dependency packages.
#[allow(clippy::too_many_lines)]
pub fn load_program_with_context(
    entry_file: &Path,
    ctx: &ProgramLoadContext,
    layout: Option<&BuildLayout>,
    bag: &mut DiagnosticBag,
) -> Option<LoadedProgram> {
    let entry_file = entry_file
        .canonicalize()
        .unwrap_or_else(|_| entry_file.to_path_buf());
    let workspace = &ctx.workspace;
    let entry_logical =
        ModulePath::from_file_path(&workspace.module_src, &entry_file, &workspace.name)
            .unwrap_or_else(|| ModulePath::new(vec![workspace.name.clone()]));

    let dep_names: Vec<&str> = ctx.dep_names();
    let mut interner = Interner::new();
    let mut pending: Vec<(ModulePath, PathBuf, phx_diagnostics::Span, u32)> = Vec::new();
    let mut loaded_paths: HashMap<String, PathBuf> = HashMap::new();
    let mut submodules = SubmoduleRegistry::default();
    let mut packages_to_validate: HashSet<(PathBuf, String)> = HashSet::new();

    pending.push((
        entry_logical.clone(),
        entry_file.clone(),
        phx_diagnostics::Span::new(0, 1),
        0,
    ));
    loaded_paths.insert(entry_logical.display(), entry_file.clone());
    packages_to_validate.insert((workspace.module_src.clone(), workspace.name.clone()));

    if workspace.package_type == PackageType::Lib {
        enqueue_package_root(
            &workspace.module_src,
            &workspace.name,
            workspace.package_type,
            &mut pending,
            &mut loaded_paths,
        );
    }

    if ctx.prelude {
        for dep in &ctx.dependencies {
            if dep.name == "std" {
                packages_to_validate.insert((dep.module_src.clone(), dep.name.clone()));
                enqueue_package_root(
                    &dep.module_src,
                    &dep.name,
                    dep.package_type,
                    &mut pending,
                    &mut loaded_paths,
                );
            }
        }
    }

    let mut modules_raw: Vec<(ModulePath, PathBuf, SourceText, Program)> = Vec::new();

    while let Some((logical, fs_path, import_span, importer_module)) = pending.pop() {
        if modules_raw
            .iter()
            .any(|(p, _, _, _)| p.display() == logical.display())
        {
            continue;
        }
        let source = match std::fs::read_to_string(&fs_path) {
            Ok(s) => s,
            Err(e) => {
                bag.push(
                    importer_module,
                    ResolveError::ModuleIo {
                        span: import_span,
                        path: fs_path.display().to_string(),
                        message: e.to_string(),
                    },
                );
                continue;
            }
        };
        let file = match parse_with_interner(&source, &mut interner) {
            Ok(f) => f,
            Err(parse_bag) => {
                let span = parse_bag
                    .errors()
                    .iter()
                    .find_map(phx_diagnostics::ParseError::span)
                    .unwrap_or(import_span);
                let failing_module = u32::try_from(modules_raw.len()).unwrap_or(u32::MAX);
                bag.push(
                    failing_module,
                    ResolveError::ModuleParse {
                        span,
                        path: fs_path.display().to_string(),
                        message: parse_bag.to_string(),
                    },
                );
                continue;
            }
        };
        let mut program = file.program;
        let compile_cfg = CompileCfg::host();
        if let Err(err) = strip_cfg(&mut program, &compile_cfg, &interner) {
            let current_module = u32::try_from(modules_raw.len()).unwrap_or(u32::MAX);
            bag.push(
                current_module,
                ResolveError::InvalidCfg {
                    span: err.span,
                    message: err.message,
                },
            );
            continue;
        }
        if let Err(err) = expand_derives(&mut program, &interner) {
            let current_module = u32::try_from(modules_raw.len()).unwrap_or(u32::MAX);
            bag.push(
                current_module,
                ResolveError::InvalidCfg {
                    span: err.span,
                    message: format!("invalid `#derive`: {}", err.message),
                },
            );
            continue;
        }
        let current_module = u32::try_from(modules_raw.len()).unwrap_or(u32::MAX);

        let submodule_parent =
            effective_submodule_parent(&logical, &fs_path, &workspace.module_src, &workspace.name);
        submodules.ingest_module(&submodule_parent.display(), &program, &interner);
        enqueue_declared_submodules(
            &submodule_parent,
            &fs_path,
            &submodules,
            &mut pending,
            &mut loaded_paths,
            bag,
            current_module,
            ctx,
            &mut packages_to_validate,
        );

        for imp in all_imports(&program) {
            let raw_target = super::graph::import_target_module(&imp.inner, &interner);
            let canonical =
                ModulePath::canonicalize_import(&raw_target, &workspace.name, &dep_names);
            let key = canonical.display();
            if key.is_empty() {
                continue;
            }
            if loaded_paths.contains_key(&key) {
                continue;
            }
            let current_pkg = logical.package_name();
            let import_pkg = canonical.package_name();
            if import_pkg == current_pkg {
                continue;
            }
            let Some(pkg) = ctx.package_for_logical(&key) else {
                bag.push(
                    current_module,
                    ResolveError::ModuleNotFound {
                        span: imp.span,
                        path: key,
                    },
                );
                continue;
            };
            let pkg_root = ModulePath::new(vec![import_pkg.to_owned()]);
            let root_key = pkg_root.display();
            if import_pkg != current_pkg && !loaded_paths.contains_key(&root_key) {
                enqueue_package_root(
                    &pkg.module_src,
                    &pkg.name,
                    pkg.package_type,
                    &mut pending,
                    &mut loaded_paths,
                );
            }
            let Some(dep_fs) = ModulePath::resolve_existing_file(
                &pkg.module_src,
                &canonical,
                &pkg.name,
                pkg.package_type,
            ) else {
                bag.push(
                    current_module,
                    ResolveError::ModuleNotFound {
                        span: imp.span,
                        path: key,
                    },
                );
                continue;
            };
            loaded_paths.insert(key.clone(), dep_fs.clone());
            if let Some(pkg) = ctx.package_for_logical(&key) {
                packages_to_validate.insert((pkg.module_src.clone(), pkg.name.clone()));
            }
            pending.push((canonical, dep_fs, imp.span, current_module));
        }

        modules_raw.push((logical, fs_path, SourceText::from(source), program));
    }

    // Filesystem layout rules apply to phoenix.toml projects, not ad-hoc `--module-src` checks.
    if layout.is_some() {
        let reporter = u32::try_from(modules_raw.len()).unwrap_or(0);
        for (module_src, package_name) in packages_to_validate {
            if package_name != workspace.name {
                continue;
            }
            validate_ambiguous_module_entries(&module_src, bag, reporter);
            validate_missing_module_entries(&module_src, bag, reporter);
            let loaded: HashSet<String> = loaded_paths.keys().cloned().collect();
            validate_orphan_files(&module_src, &package_name, &loaded, bag, reporter);
        }
    }

    if bag.has_errors() {
        return None;
    }

    let root_key = entry_logical.display();
    let module_count = modules_raw.len();
    let mut id_for_path: HashMap<String, ModuleId> = HashMap::new();
    let mut modules: Vec<LoadedModule> = Vec::with_capacity(module_count);

    for (i, (logical, fs, source, program)) in modules_raw.into_iter().enumerate() {
        let id = ModuleId::from_raw(u32::try_from(i).unwrap_or(u32::MAX));
        id_for_path.insert(logical.display(), id);
        modules.push(LoadedModule {
            id,
            logical_path: logical,
            filesystem: fs,
            source,
            program,
        });
    }

    let path_index: HashMap<String, ModuleId> =
        id_for_path.iter().map(|(k, v)| (k.clone(), *v)).collect();

    let mut edges = Vec::new();
    for m in &modules {
        let mut local_bag = DiagnosticBag::new();
        let module_imports: Vec<_> = all_imports(&m.program).into_iter().cloned().collect();
        let mut e = collect_edges(
            m.id,
            &module_imports,
            &path_index,
            &interner,
            &workspace.name,
            &dep_names,
            &mut local_bag,
        );
        for err in local_bag.into_errors() {
            bag.push_located(err);
        }
        edges.append(&mut e);
    }

    if bag.has_errors() {
        return None;
    }

    let root_id = id_for_path
        .get(&root_key)
        .copied()
        .or_else(|| id_for_path.values().next().copied())?;

    let _order = topo_sort_with_pxi_escape(
        module_count,
        &edges,
        root_id,
        &modules,
        layout,
        &workspace.name,
        &dep_names,
        bag,
    )?;

    // Keep `modules[id]` at index `id.index()` — resolve/import code indexes by `ModuleId`.
    let mut indexed: Vec<Option<LoadedModule>> = (0..module_count).map(|_| None).collect();
    for m in modules {
        let idx = m.id.index() as usize;
        indexed[idx] = Some(m);
    }
    let mut modules: Vec<LoadedModule> = Vec::with_capacity(module_count);
    for (i, slot) in indexed.into_iter().enumerate() {
        let Some(m) = slot else {
            bag.push(
                0,
                ResolveError::ModuleNotFound {
                    span: phx_diagnostics::Span::new(0, 1),
                    path: format!("missing module slot {i} after load"),
                },
            );
            return None;
        };
        debug_assert_eq!(m.id.index() as usize, i);
        modules.push(m);
    }

    let path_index: HashMap<String, ModuleId> = modules
        .iter()
        .map(|m| (m.logical_path.display(), m.id))
        .collect();

    Some(LoadedProgram {
        interner,
        modules,
        root: root_id,
        path_index,
        package_name: workspace.name.clone(),
        package_type: workspace.package_type,
        dep_package_names: ctx.dependencies.iter().map(|d| d.name.clone()).collect(),
        build_layout: layout.cloned(),
        prelude_enabled: ctx.prelude,
        submodules,
    })
}

fn enqueue_package_root(
    module_src: &Path,
    package_name: &str,
    package_type: PackageType,
    pending: &mut Vec<(ModulePath, PathBuf, phx_diagnostics::Span, u32)>,
    loaded_paths: &mut HashMap<String, PathBuf>,
) {
    let root_logical = ModulePath::new(vec![package_name.to_owned()]);
    let key = root_logical.display();
    if loaded_paths.contains_key(&key) {
        return;
    }
    let Some(fs) =
        ModulePath::resolve_existing_file(module_src, &root_logical, package_name, package_type)
    else {
        return;
    };
    loaded_paths.insert(key, fs.clone());
    pending.push((root_logical, fs, phx_diagnostics::Span::new(0, 1), 0));
}

#[allow(clippy::too_many_arguments)]
fn enqueue_declared_submodules(
    parent_logical: &ModulePath,
    parent_fs: &Path,
    submodules: &SubmoduleRegistry,
    pending: &mut Vec<(ModulePath, PathBuf, phx_diagnostics::Span, u32)>,
    loaded_paths: &mut HashMap<String, PathBuf>,
    bag: &mut DiagnosticBag,
    importer_module: u32,
    ctx: &ProgramLoadContext,
    packages_to_validate: &mut HashSet<(PathBuf, String)>,
) {
    let parent_key = parent_logical.display();
    let Some(decls) = submodules.decls.get(&parent_key) else {
        return;
    };
    let Some(pkg) = ctx.package_for_logical(&parent_key) else {
        return;
    };
    packages_to_validate.insert((pkg.module_src.clone(), pkg.name.clone()));
    for decl in decls {
        let Some((child_logical, child_fs)) = resolve_submodule_file(
            &pkg.module_src,
            parent_fs,
            parent_logical,
            &decl.name,
            &pkg.name,
            pkg.package_type,
        ) else {
            bag.push(
                importer_module,
                ResolveError::ModuleNotFound {
                    span: decl.span,
                    path: format!("{parent_key}::{}", decl.name),
                },
            );
            continue;
        };
        let key = child_logical.display();
        if loaded_paths.contains_key(&key) {
            continue;
        }
        loaded_paths.insert(key, child_fs.clone());
        pending.push((child_logical, child_fs, decl.span, importer_module));
    }
}

fn infer_package_name(module_root: &Path) -> String {
    module_root
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .unwrap_or("app")
        .to_owned()
}

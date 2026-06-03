//! Load all modules reachable from the entry file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use phx_diagnostics::{DiagnosticBag, ResolveError};
use phx_syntax::{Interner, Program, parse_with_interner};

use super::graph::{collect_edges, topo_sort_with_pxi_escape};
use super::load_context::CrateLoadContext;
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
    pub source: String,
    /// Parsed program (imports + items).
    pub program: Program,
}

/// All modules in a crate before resolve.
#[derive(Debug, Clone)]
pub struct LoadedCrate {
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
}

/// Loads the module graph starting at `entry_file` under `module_root` (single-package fallback).
///
/// # Errors
///
/// Returns I/O or parse errors via `bag`; also records module-not-found and cycles.
pub fn load_crate(
    entry_file: &Path,
    module_root: &Path,
    bag: &mut DiagnosticBag,
) -> Option<LoadedCrate> {
    let ctx = CrateLoadContext {
        workspace: super::load_context::PackageRoot {
            name: infer_package_name(module_root),
            module_src: module_root
                .canonicalize()
                .unwrap_or_else(|_| module_root.to_path_buf()),
            package_type: PackageType::Bin,
        },
        dependencies: Vec::new(),
    };
    load_crate_with_context(entry_file, &ctx, None, bag)
}

/// Loads a crate from `entry_file` using workspace + dependency packages.
pub fn load_crate_with_context(
    entry_file: &Path,
    ctx: &CrateLoadContext,
    layout: Option<&BuildLayout>,
    bag: &mut DiagnosticBag,
) -> Option<LoadedCrate> {
    let entry_file = entry_file.canonicalize().unwrap_or_else(|_| entry_file.to_path_buf());
    let workspace = &ctx.workspace;
    let entry_logical = ModulePath::from_file_path(
        &workspace.module_src,
        &entry_file,
        &workspace.name,
    )
    .unwrap_or_else(|| ModulePath::new(vec![workspace.name.clone()]));

    let dep_names: Vec<&str> = ctx.dep_names();
    let mut interner = Interner::new();
    let mut pending: Vec<(ModulePath, PathBuf)> = Vec::new();
    let mut loaded_paths: HashMap<String, PathBuf> = HashMap::new();

    pending.push((entry_logical.clone(), entry_file.clone()));
    loaded_paths.insert(entry_logical.display(), entry_file.clone());

    let mut modules_raw: Vec<(ModulePath, PathBuf, String, Program)> = Vec::new();

    while let Some((logical, fs_path)) = pending.pop() {
        if modules_raw
            .iter()
            .any(|(p, _, _, _)| p.display() == logical.display())
        {
            continue;
        }
        let source = match std::fs::read_to_string(&fs_path) {
            Ok(s) => s,
            Err(e) => {
                bag.push(ResolveError::ModuleIo {
                    span: phx_diagnostics::Span::new(0, 0),
                    path: fs_path.display().to_string(),
                    message: e.to_string(),
                });
                continue;
            }
        };
        let file = match parse_with_interner(&source, &mut interner) {
            Ok(f) => f,
            Err(e) => {
                bag.push(ResolveError::ModuleParse {
                    span: phx_diagnostics::Span::new(0, 0),
                    path: fs_path.display().to_string(),
                    message: e.to_string(),
                });
                continue;
            }
        };
        let program = file.program;

        for imp in &program.imports {
            let raw_target = super::graph::import_target_module(&imp.inner, &interner);
            let canonical = ModulePath::canonicalize_import(
                &raw_target,
                &workspace.name,
                &dep_names,
            );
            let key = canonical.display();
            if key.is_empty() {
                continue;
            }
            if loaded_paths.contains_key(&key) {
                continue;
            }
            let Some(pkg) = ctx.package_for_logical(&key) else {
                bag.push(ResolveError::ModuleNotFound {
                    span: imp.span,
                    path: key,
                });
                continue;
            };
            let Some(dep_fs) =
                ModulePath::resolve_existing_file(&pkg.module_src, &canonical, &pkg.name, pkg.package_type)
            else {
                bag.push(ResolveError::ModuleNotFound {
                    span: imp.span,
                    path: key,
                });
                continue;
            };
            loaded_paths.insert(key.clone(), dep_fs.clone());
            pending.push((canonical, dep_fs));
        }

        modules_raw.push((logical, fs_path, source, program));
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

    let path_index: HashMap<String, ModuleId> = id_for_path
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();

    let mut edges = Vec::new();
    for m in &modules {
        let mut local_bag = DiagnosticBag::new();
        let mut e = collect_edges(
            m.id,
            &m.program.imports,
            &path_index,
            &interner,
            &workspace.name,
            &dep_names,
            &mut local_bag,
        );
        for err in local_bag.into_errors() {
            bag.push(err);
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

    let order = topo_sort_with_pxi_escape(
        module_count,
        &edges,
        root_id,
        &modules,
        layout,
        bag,
    )?;
    let index_map: HashMap<ModuleId, usize> = modules
        .iter()
        .map(|m| (m.id, m.id.index() as usize))
        .collect();
    let mut sorted = Vec::with_capacity(modules.len());
    for id in order {
        let idx = index_map[&id];
        sorted.push(modules[idx].clone());
    }

    let path_index: HashMap<String, ModuleId> = sorted
        .iter()
        .map(|m| (m.logical_path.display(), m.id))
        .collect();

    Some(LoadedCrate {
        interner,
        modules: sorted,
        root: root_id,
        path_index,
        package_name: workspace.name.clone(),
        package_type: workspace.package_type,
        dep_package_names: ctx.dependencies.iter().map(|d| d.name.clone()).collect(),
    })
}

fn infer_package_name(module_root: &Path) -> String {
    module_root
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .unwrap_or("app")
        .to_owned()
}

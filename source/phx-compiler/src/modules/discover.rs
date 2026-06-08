//! Submodule declarations and filesystem discovery (V0-061).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use phx_diagnostics::{DiagnosticBag, ResolveError, Span};
use phx_syntax::Interner;
use phx_syntax::ast::decl::{Program, TopLevelDecl};
use phx_syntax::ast::ident::PathSegment;

use super::path::ModulePath;
use crate::project::PackageType;

/// One `mod name` declaration in a module file.
#[derive(Debug, Clone)]
pub struct SubmoduleDecl {
    /// Submodule stem (`from_io`).
    pub name: String,
    /// `pub` on the declaration.
    pub exported: bool,
    /// Declaration span.
    pub span: Span,
}

/// One `pub reexport :: path` declaration.
#[derive(Debug, Clone)]
pub struct ReexportDecl {
    /// Path segments after `::` (`Item` or `child::Item`).
    pub segments: Vec<String>,
    /// Declaration span.
    pub span: Span,
}

/// Submodule graph collected while loading a program.
#[derive(Debug, Clone, Default)]
pub struct SubmoduleRegistry {
    /// `child_logical_display` → parent logical display.
    pub parent: HashMap<String, String>,
    /// Child logical paths declared `pub mod`.
    pub public_children: HashSet<String>,
    /// Per parent logical path, ordered declarations.
    pub decls: HashMap<String, Vec<SubmoduleDecl>>,
    /// Per parent logical path, reexport declarations.
    pub reexports: HashMap<String, Vec<ReexportDecl>>,
}

impl SubmoduleRegistry {
    /// Records declarations from `program` owned by `parent_logical`.
    pub fn ingest_module(&mut self, parent_logical: &str, program: &Program, interner: &Interner) {
        let decls = self.decls.entry(parent_logical.to_owned()).or_default();
        let reexports = self.reexports.entry(parent_logical.to_owned()).or_default();
        for item in &program.items {
            match &item.inner.decl {
                TopLevelDecl::Mod { name } => {
                    let stem = interner.resolve(name.symbol).to_owned();
                    decls.push(SubmoduleDecl {
                        name: stem.clone(),
                        exported: item.inner.pub_,
                        span: item.span,
                    });
                    let child = format!("{parent_logical}::{stem}");
                    self.parent.insert(child.clone(), parent_logical.to_owned());
                    if item.inner.pub_ {
                        self.public_children.insert(child);
                    }
                }
                TopLevelDecl::Reexport { path } => {
                    let segments: Vec<String> = path
                        .segments
                        .iter()
                        .map(|seg| match seg {
                            PathSegment::Ident(id) => interner.resolve(id.symbol).to_owned(),
                            PathSegment::Type(type_name) => {
                                interner.resolve(type_name.symbol).to_owned()
                            }
                        })
                        .collect();
                    reexports.push(ReexportDecl {
                        segments,
                        span: item.span,
                    });
                }
                _ => {}
            }
        }
    }
}

/// Logical parent for `mod` declarations when the file is an ad-hoc package-root entry.
#[must_use]
pub fn effective_submodule_parent(
    logical: &ModulePath,
    parent_fs: &Path,
    module_src: &Path,
    package_name: &str,
) -> ModulePath {
    let parent_dir = parent_fs.parent().unwrap_or(module_src);
    if parent_dir == module_src {
        let stem = parent_fs.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if stem != "main" && stem != "lib" && stem != "mod" {
            return ModulePath::new(vec![package_name.to_owned()]);
        }
    }
    logical.clone()
}

/// Directory that holds submodule source files for `parent_fs`.
#[must_use]
pub fn submodule_search_dir(parent_fs: &Path, module_src: &Path) -> PathBuf {
    let parent_dir = parent_fs.parent().unwrap_or_else(|| Path::new("."));
    let stem = parent_fs.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if stem == "mod" || stem == "main" || stem == "lib" {
        return parent_dir.to_path_buf();
    }
    // Ad-hoc entry files at `module_src` root (e.g. test fixtures) declare package-level children.
    if parent_dir == module_src {
        return parent_dir.to_path_buf();
    }
    parent_dir.join(stem)
}

/// Resolves `child` declared in `parent_logical` to filesystem path and logical path.
#[must_use]
pub fn resolve_submodule_file(
    module_src: &Path,
    parent_fs: &Path,
    parent_logical: &ModulePath,
    child_stem: &str,
    package_name: &str,
    _package_type: PackageType,
) -> Option<(ModulePath, PathBuf)> {
    let search = submodule_search_dir(parent_fs, module_src);
    let mut inner: Vec<String> = parent_logical.within_package(package_name).to_vec();
    inner.push(child_stem.to_owned());
    let child_logical = ModulePath::new({
        let mut segs = vec![package_name.to_owned()];
        segs.extend(inner);
        segs
    });
    let direct = search.join(format!("{child_stem}.phx"));
    let mod_file = search.join(child_stem).join("mod.phx");
    let fs = if direct.is_file() {
        direct
    } else if mod_file.is_file() {
        mod_file
    } else {
        return None;
    };
    Some((child_logical, fs))
}

/// Reports `.phx` files under `module_src` not reachable via `loaded` paths.
pub fn validate_orphan_files(
    module_src: &Path,
    package_name: &str,
    loaded: &HashSet<String>,
    bag: &mut DiagnosticBag,
    reporter_module: u32,
) {
    let mut disk = Vec::new();
    collect_phx_files(module_src, &mut disk);
    for path in disk {
        let Ok(rel) = path.strip_prefix(module_src) else {
            continue;
        };
        // Root-level `.phx` files (`main`, `lib`, ad-hoc entries) are not submodule-managed.
        if rel.components().count() <= 1 {
            continue;
        }
        let Some(logical) = ModulePath::from_file_path(module_src, &path, package_name) else {
            continue;
        };
        let key = logical.display();
        if !loaded.contains(&key) {
            bag.push(
                reporter_module,
                ResolveError::OrphanModuleFile {
                    span: Span::new(0, 1),
                    path: path.display().to_string(),
                    hint: format!(
                        "declare `mod {name};` in the parent `mod.phx` or remove the file",
                        name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?")
                    ),
                },
            );
        }
    }
}

/// Reports `foo.phx` + `foo/mod.phx` pairs under `module_src`.
pub fn validate_ambiguous_module_entries(
    module_src: &Path,
    bag: &mut DiagnosticBag,
    reporter_module: u32,
) {
    let mut disk = Vec::new();
    collect_phx_files(module_src, &mut disk);
    let mut seen: HashSet<String> = HashSet::new();
    for path in disk {
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if file_name != "mod.phx" {
            continue;
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let dir_name = parent.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let sibling = parent
            .parent()
            .unwrap_or(module_src)
            .join(format!("{dir_name}.phx"));
        if sibling.is_file() {
            let key = sibling.display().to_string();
            if seen.insert(key.clone()) {
                bag.push(
                    reporter_module,
                    ResolveError::AmbiguousModuleEntry {
                        span: Span::new(0, 1),
                        flat: sibling.display().to_string(),
                        module_dir: path.display().to_string(),
                    },
                );
            }
        }
    }
}

/// Reports a directory with `.phx` children but no module entry file.
pub fn validate_missing_module_entries(
    module_src: &Path,
    bag: &mut DiagnosticBag,
    reporter_module: u32,
) {
    let Ok(entries) = std::fs::read_dir(module_src) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let has_phx_child = std::fs::read_dir(&path).ok().is_some_and(|rd| {
            rd.flatten()
                .any(|e| e.path().extension().is_some_and(|ext| ext == "phx"))
        });
        if !has_phx_child {
            continue;
        }
        let dir_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let flat = module_src.join(format!("{dir_name}.phx"));
        let mod_entry = path.join("mod.phx");
        if !flat.is_file() && !mod_entry.is_file() {
            bag.push(
                reporter_module,
                ResolveError::MissingModuleEntry {
                    span: Span::new(0, 1),
                    dir: path.display().to_string(),
                },
            );
        }
    }
}

/// Returns whether `importer_logical` may import symbols from `target_logical`.
#[must_use]
pub fn is_module_importable(
    importer_logical: &str,
    target_logical: &str,
    reg: &SubmoduleRegistry,
) -> bool {
    let importer_pkg = importer_logical
        .split("::")
        .next()
        .unwrap_or(importer_logical);
    let target_pkg = target_logical.split("::").next().unwrap_or(target_logical);
    if importer_pkg == target_pkg {
        return true;
    }
    if target_logical == target_pkg {
        return true;
    }
    let mut current = target_logical.to_string();
    while current != target_pkg {
        if !reg.public_children.contains(&current) {
            return false;
        }
        let Some(parent) = reg.parent.get(&current) else {
            return false;
        };
        current.clone_from(parent);
    }
    true
}

fn collect_phx_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_phx_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "phx") {
            out.push(path);
        }
    }
}

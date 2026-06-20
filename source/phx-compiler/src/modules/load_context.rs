//! Multi-package program loading (workspace + path dependencies).
//!
//! ## Pass role
//!
//! Describes which package roots participate in one compile: the workspace crate and its path
//! dependencies. [`ProgramLoadContext`] is built from [`ProjectConfig`] or standalone CLI flags
//! and passed to [`super::load_program_with_context`] so the loader knows where to resolve each
//! logical path's first segment (`pkg::a::b` → package `pkg`).
//!
//! ## Entry points
//!
//! - [`ProgramLoadContext::from_config`] — `phoenix.toml` workspace + declared deps
//! - [`ProgramLoadContext::from_standalone`] — ad-hoc `--module-src` with optional path deps
//! - [`ProgramLoadContext::package_for_logical`] — resolve first path segment to a package root
//!
//! Path dependency validation (lib package type, name/key match) runs in [`Self::from_standalone`];
//! [`Self::from_config`] loads dependency configs best-effort and does not fail the whole context
//! when a declared dep directory is missing.

use std::path::PathBuf;

use crate::project::{PackageType, ProjectConfig, ProjectError};

/// One package root participating in a load.
///
/// Maps a Phoenix package name to its on-disk `module_src` directory and package kind (`bin` or
/// `lib`). The loader uses this to canonicalize `#import` paths and locate `.phx` files under each
/// dependency tree.
#[derive(Debug, Clone)]
pub struct PackageRoot {
    /// Package name from `project.name` in `phoenix.toml`, or inferred from the directory name for
    /// standalone loads.
    pub name: String,
    /// Absolute path to the directory containing module sources (`module_src` from config, or the
    /// canonicalized `--module-src` root for standalone workspace packages).
    pub module_src: PathBuf,
    /// Whether this package is a binary entry (`bin`) or library (`lib`).
    pub package_type: PackageType,
}

impl PackageRoot {
    /// Builds a package root from a loaded [`ProjectConfig`].
    ///
    /// Uses [`ProjectConfig::module_root`] for `module_src` and copies `name` and
    /// `package_type` from the config.
    #[must_use]
    pub fn from_config(config: &ProjectConfig) -> Self {
        Self {
            name: config.name.clone(),
            module_src: config.module_root(),
            package_type: config.package_type,
        }
    }
}

/// Workspace package plus path dependencies for one compile load.
///
/// Bundles every package whose modules may appear in a single program graph. The workspace is the
/// package being compiled; dependencies are path-linked crates listed in `phoenix.toml` or passed
/// on the CLI. [`Self::prelude`] controls whether the std prelude is injected during resolve.
#[derive(Debug, Clone)]
pub struct ProgramLoadContext {
    /// Primary package being compiled (entry module lives under this root).
    pub workspace: PackageRoot,
    /// Path dependencies, in declaration order from `project.dependencies`.
    pub dependencies: Vec<PackageRoot>,
    /// When `true`, inject std prelude bindings into each module during resolve. Defaults to
    /// `project.prelude && has_std` for config loads; standalone loads set this to `false`.
    pub prelude: bool,
}

impl ProgramLoadContext {
    /// Builds load context from a project config.
    ///
    /// Loads each declared path dependency when its directory contains a valid `phoenix.toml`;
    /// missing or unreadable deps are skipped silently. Prelude is enabled when
    /// `config.prelude` is set and std is available (`bundle_std` or a `std` dependency entry).
    #[must_use]
    pub fn from_config(config: &ProjectConfig) -> Self {
        let mut dependencies = Vec::new();
        let has_std = config.bundle_std || config.dependencies.contains_key("std");
        for dep in config.dependencies.values() {
            let dep_root = config.root.join(&dep.path);
            if let Ok(dep_cfg) = ProjectConfig::load(&dep_root) {
                dependencies.push(PackageRoot::from_config(&dep_cfg));
            }
        }
        let prelude = config.prelude && has_std;
        Self {
            workspace: PackageRoot::from_config(config),
            dependencies,
            prelude,
        }
    }

    /// Returns the names of all path-dependency packages.
    ///
    /// Order matches [`Self::dependencies`]; used when canonicalizing cross-package import paths.
    #[must_use]
    pub fn dep_names(&self) -> Vec<&str> {
        self.dependencies.iter().map(|d| d.name.as_str()).collect()
    }

    /// Finds which package owns a canonical logical path.
    ///
    /// Uses the first `::` segment of `logical` (e.g. `my_lib::foo` → package `my_lib`). Returns
    /// the workspace root when the segment matches [`Self::workspace`], otherwise the matching
    /// dependency root, or `None` when no package name matches.
    #[must_use]
    pub fn package_for_logical(&self, logical: &str) -> Option<&PackageRoot> {
        let first = logical.split("::").next()?;
        if first == self.workspace.name {
            return Some(&self.workspace);
        }
        self.dependencies.iter().find(|d| d.name == first)
    }

    /// Builds load context for a standalone CLI invocation (no `phoenix.toml`).
    ///
    /// Canonicalizes `module_root` for the workspace package. Each path dependency must either
    /// load as a `lib` package from `phoenix.toml` or expose a `lib.phx` at the dependency root.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::Invalid`] when a dependency is not a library package, when the
    /// dependency key does not match `project.name` in its config, or when neither `phoenix.toml`
    /// nor `lib.phx` exists at the dependency path.
    pub fn from_standalone(
        module_root: &std::path::Path,
        package_name: Option<String>,
        path_deps: &[(String, PathBuf)],
    ) -> Result<Self, ProjectError> {
        let module_src = module_root
            .canonicalize()
            .unwrap_or_else(|_| module_root.to_path_buf());
        let name = package_name.unwrap_or_else(|| infer_package_name(&module_src));
        let workspace = PackageRoot {
            name,
            module_src,
            package_type: PackageType::Bin,
        };
        let mut dependencies = Vec::new();
        for (dep_name, dep_path) in path_deps {
            let root = dep_path.canonicalize().unwrap_or_else(|_| dep_path.clone());
            if let Ok(cfg) = ProjectConfig::load(&root) {
                if cfg.package_type != PackageType::Lib {
                    return Err(ProjectError::Invalid {
                        message: format!(
                            "dependency `{dep_name}` at {} must be `type = \"lib\"`",
                            root.display()
                        ),
                    });
                }
                if cfg.name != *dep_name {
                    return Err(ProjectError::Invalid {
                        message: format!(
                            "dependency key `{dep_name}` does not match project name `{}` in {}",
                            cfg.name,
                            root.display()
                        ),
                    });
                }
                dependencies.push(PackageRoot::from_config(&cfg));
            } else {
                let lib_entry = root.join("lib.phx");
                if !lib_entry.is_file() {
                    return Err(ProjectError::Invalid {
                        message: format!(
                            "dependency `{dep_name}` at {} requires lib.phx or phoenix.toml",
                            root.display()
                        ),
                    });
                }
                dependencies.push(PackageRoot {
                    name: dep_name.clone(),
                    module_src: root,
                    package_type: PackageType::Lib,
                });
            }
        }
        Ok(Self {
            workspace,
            dependencies,
            prelude: false,
        })
    }
}

fn infer_package_name(module_root: &std::path::Path) -> String {
    module_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("app")
        .to_owned()
}

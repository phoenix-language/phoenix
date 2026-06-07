//! Multi-package program loading (workspace + path dependencies).

use std::path::PathBuf;

use crate::project::{PackageType, ProjectConfig, ProjectError};

/// One package root participating in a load.
#[derive(Debug, Clone)]
pub struct PackageRoot {
    /// `project.name`
    pub name: String,
    /// Absolute `module_src` directory.
    pub module_src: PathBuf,
    /// `bin` or `lib`.
    pub package_type: PackageType,
}

impl PackageRoot {
    /// Builds from a loaded [`ProjectConfig`].
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
#[derive(Debug, Clone)]
pub struct ProgramLoadContext {
    /// Package being compiled.
    pub workspace: PackageRoot,
    /// Path dependencies (`project.name` order).
    pub dependencies: Vec<PackageRoot>,
}

impl ProgramLoadContext {
    /// Builds load context from project config (does not validate deps).
    #[must_use]
    pub fn from_config(config: &ProjectConfig) -> Self {
        let mut dependencies = Vec::new();
        for dep in config.dependencies.values() {
            let dep_root = config.root.join(&dep.path);
            if let Ok(dep_cfg) = ProjectConfig::load(&dep_root) {
                dependencies.push(PackageRoot::from_config(&dep_cfg));
            }
        }
        Self {
            workspace: PackageRoot::from_config(config),
            dependencies,
        }
    }

    /// Dependency package names.
    #[must_use]
    pub fn dep_names(&self) -> Vec<&str> {
        self.dependencies.iter().map(|d| d.name.as_str()).collect()
    }

    /// Finds which package owns a canonical logical path (first segment).
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
    /// # Errors
    ///
    /// Returns [`ProjectError`] when a path dependency cannot be loaded as a library package.
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

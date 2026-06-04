//! Multi-package crate loading (workspace + path dependencies).

use std::path::PathBuf;

use crate::project::{PackageType, ProjectConfig};

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

/// Workspace crate plus path dependencies for import resolution.
#[derive(Debug, Clone)]
pub struct CrateLoadContext {
    /// Package being compiled.
    pub workspace: PackageRoot,
    /// Path dependencies (`project.name` order).
    pub dependencies: Vec<PackageRoot>,
}

impl CrateLoadContext {
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
    pub fn package_for_logical(&self, logical: &str) -> Option<&PackageRoot> {
        let first = logical.split("::").next()?;
        if first == self.workspace.name {
            return Some(&self.workspace);
        }
        self.dependencies.iter().find(|d| d.name == first)
    }
}

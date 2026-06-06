//! Artifact paths under `build/`.

use std::path::PathBuf;

use super::config::ProjectConfig;

/// Paths for one logical module's build artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleArtifacts {
    /// `build/pxi/.../*.pxi`
    pub pxi: PathBuf,
    /// `build/phx0/.../*.phx0`
    pub phx0: PathBuf,
}

/// Build directory layout helpers.
#[derive(Debug, Clone)]
pub struct BuildLayout {
    build_root: PathBuf,
}

impl BuildLayout {
    /// Artifact root (`build/` or `build/deps/{name}/`).
    #[must_use]
    pub fn build_root(&self) -> &std::path::Path {
        &self.build_root
    }

    /// Creates layout for `config.build_root()`.
    #[must_use]
    pub fn new(config: &ProjectConfig) -> Self {
        Self {
            build_root: config.build_root(),
        }
    }

    /// Layout for a dependency built under `build/deps/{dep_name}/`.
    #[must_use]
    pub fn for_dependency(workspace: &ProjectConfig, dep_name: &str) -> Self {
        Self {
            build_root: workspace.build_root().join("deps").join(dep_name),
        }
    }

    /// `build/manifest.json`
    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.build_root.join("manifest.json")
    }

    /// `build/bin/<name>.phx0`
    #[must_use]
    pub fn bin_path(&self, name: &str) -> PathBuf {
        self.build_root.join("bin").join(format!("{name}.phx0"))
    }

    /// `build/lib/<name>.phx0`
    #[must_use]
    pub fn lib_path(&self, name: &str) -> PathBuf {
        self.build_root.join("lib").join(format!("{name}.phx0"))
    }

    /// Per-module `.pxi` and `.phx0` paths mirroring `::` as `/`.
    #[must_use]
    pub fn module_artifacts(&self, logical_path: &str) -> ModuleArtifacts {
        let rel = logical_to_rel(logical_path);
        ModuleArtifacts {
            pxi: self.build_root.join("pxi").join(&rel).with_extension("pxi"),
            phx0: self
                .build_root
                .join("phx0")
                .join(&rel)
                .with_extension("phx0"),
        }
    }

    /// Artifact paths for `logical_path`, using `build/deps/{dep}/` when the first path segment is a path dependency.
    #[must_use]
    pub fn module_artifacts_resolved(
        &self,
        logical_path: &str,
        workspace_package: &str,
        dep_names: &[&str],
    ) -> ModuleArtifacts {
        let first = logical_path.split("::").next().unwrap_or("");
        if !first.is_empty() && first != workspace_package && dep_names.contains(&first) {
            let dep_root = self.build_root.join("deps").join(first);
            BuildLayout {
                build_root: dep_root,
            }
            .module_artifacts(logical_path)
        } else {
            self.module_artifacts(logical_path)
        }
    }

    /// Ensures workspace build subdirectories exist.
    ///
    /// # Errors
    ///
    /// Returns I/O errors from `create_dir_all`.
    pub fn ensure_workspace_dirs(
        &self,
        package_type: super::config::PackageType,
    ) -> std::io::Result<()> {
        std::fs::create_dir_all(self.build_root.join("pxi"))?;
        std::fs::create_dir_all(self.build_root.join("phx0"))?;
        std::fs::create_dir_all(self.build_root.join("deps"))?;
        match package_type {
            super::config::PackageType::Bin => {
                std::fs::create_dir_all(self.build_root.join("bin"))?;
            }
            super::config::PackageType::Lib => {
                std::fs::create_dir_all(self.build_root.join("lib"))?;
            }
        }
        Ok(())
    }

    /// Ensures dependency artifact directories exist.
    ///
    /// # Errors
    ///
    /// I/O errors from `create_dir_all`.
    pub fn ensure_dep_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.build_root.join("pxi"))?;
        std::fs::create_dir_all(self.build_root.join("phx0"))?;
        std::fs::create_dir_all(self.build_root.join("lib"))?;
        Ok(())
    }
}

fn logical_to_rel(logical: &str) -> PathBuf {
    logical.split("::").collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_paths() {
        let layout = BuildLayout {
            build_root: PathBuf::from("/p/build"),
        };
        let a = layout.module_artifacts("myapp::util::math");
        assert!(a.pxi.ends_with("myapp/util/math.pxi"));
        assert!(a.phx0.ends_with("myapp/util/math.phx0"));
    }

    #[test]
    fn dependency_artifact_paths() {
        let layout = BuildLayout {
            build_root: PathBuf::from("/p/build"),
        };
        let a = layout.module_artifacts_resolved("math::util", "app", &["math"]);
        assert!(a.pxi.ends_with("build/deps/math/pxi/math/util.pxi"));
    }
}

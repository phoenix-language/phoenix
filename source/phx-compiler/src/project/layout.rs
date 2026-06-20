//! Build directory layout — artifact paths under `build/` (M2).
//!
//! [`BuildLayout`] maps logical module paths (`app::util::math`) to on-disk `.pxi` interface
//! files and `.phx0` object bytecode under the workspace [`ProjectConfig::build_root`] or a
//! path dependency's `build/deps/{name}/` subtree. The build driver ([`crate::build`]) and
//! module loader use these helpers when emitting artifacts, checking incremental staleness, and
//! resolving cross-crate module paths.
//!
//! ## Inputs and outputs
//!
//! - **Input:** A [`ProjectConfig`] (workspace or dependency), a logical module path string
//!   (`::`-separated segments), and optional dependency name lists for cross-crate resolution.
//! - **Output:** Absolute [`PathBuf`] locations for manifest, linked binaries, and per-module
//!   [`ModuleArtifacts`].
//!
//! ## Directory tree
//!
//! Workspace root (`config.build_root()`, typically `<project>/build/`):
//!
//! ```text
//! build/
//! ├── manifest.json          # incremental rebuild metadata
//! ├── pxi/                     # interface files; :: → /
//! │   └── myapp/util/math.pxi
//! ├── phx0/                    # per-module object bytecode
//! │   └── myapp/util/math.phx0
//! ├── bin/                     # PackageType::Bin linked output
//! │   └── myapp.phx0
//! ├── lib/                     # PackageType::Lib linked output
//! │   └── mylib.phx0
//! └── deps/{name}/             # prebuilt path-dependency artifacts
//!     ├── manifest.json
//!     ├── pxi/
//!     ├── phx0/
//!     └── lib/
//! ```
//!
//! Path dependencies are built under `build/deps/{dep_name}/` with the same internal layout
//! as the workspace root (see [`Self::for_dependency`]).
//!
//! ## Logical path mapping
//!
//! Logical paths mirror Phoenix module namespaces: `myapp::util::math` becomes
//! `pxi/myapp/util/math.pxi` and `phx0/myapp/util/math.phx0`. The first segment is usually
//! the package name; [`Self::module_artifacts_resolved`] redirects to `build/deps/{first}/`
//! when that segment names a path dependency.
//!
//! ## Entry points
//!
//! - [`BuildLayout::new`] — workspace artifact root
//! - [`BuildLayout::for_dependency`] — dependency subtree under `build/deps/{name}/`
//! - [`BuildLayout::module_artifacts`] / [`BuildLayout::module_artifacts_resolved`] — per-module paths
//! - [`BuildLayout::ensure_workspace_dirs`] / [`BuildLayout::ensure_dep_dirs`] — create output dirs

use std::path::PathBuf;

use super::config::ProjectConfig;

/// Paths for one logical module's build artifacts.
///
/// Returned by [`BuildLayout::module_artifacts`] and [`BuildLayout::module_artifacts_resolved`].
/// Both paths share the same relative directory structure; only the root prefix
/// (`build/` vs `build/deps/{name}/`) differs for path-dependency modules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleArtifacts {
    /// Interface file path: `{build_root}/pxi/{segments}.pxi`.
    ///
    /// Written by the build driver after type-check; consumed for incremental staleness
    /// and cross-crate symbol visibility.
    pub pxi: PathBuf,
    /// Object bytecode path: `{build_root}/phx0/{segments}.phx0`.
    ///
    /// Per-module lowered/linked object before the final workspace link step.
    pub phx0: PathBuf,
}

/// Build directory layout helpers.
///
/// Holds a single artifact root ([`Self::build_root`]) and computes derived paths beneath it.
/// Construct with [`Self::new`] for the workspace crate or [`Self::for_dependency`] for a path
/// dependency's `build/deps/{name}/` subtree.
///
/// Instances are cheap to clone; they contain no I/O state.
#[derive(Debug, Clone)]
pub struct BuildLayout {
    build_root: PathBuf,
}

impl BuildLayout {
    /// Artifact root directory (`build/` or `build/deps/{name}/`).
    ///
    /// All other path helpers join beneath this prefix.
    #[must_use]
    pub fn build_root(&self) -> &std::path::Path {
        &self.build_root
    }

    /// Creates layout for the workspace [`ProjectConfig::build_root`].
    ///
    /// Equivalent to `BuildLayout { build_root: config.build_root() }`.
    #[must_use]
    pub fn new(config: &ProjectConfig) -> Self {
        Self {
            build_root: config.build_root(),
        }
    }

    /// Layout for a path dependency built under `build/deps/{dep_name}/`.
    ///
    /// Uses the **consumer** workspace [`ProjectConfig::build_root`] as the parent; artifact
    /// paths inside the dependency mirror the workspace layout (`pxi/`, `phx0/`, `lib/`).
    ///
    /// `dep_name` must match the dependency key in `[dependencies]` and the depended package's
    /// `project.name` (see [`ProjectConfig::validate`]).
    #[must_use]
    pub fn for_dependency(workspace: &ProjectConfig, dep_name: &str) -> Self {
        Self {
            build_root: workspace.build_root().join("deps").join(dep_name),
        }
    }

    /// Incremental rebuild manifest: `build/manifest.json`.
    ///
    /// Records per-module source hashes, `.pxi` paths, and artifact metadata for cache hits.
    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.build_root.join("manifest.json")
    }

    /// Linked executable output: `build/bin/{name}.phx0`.
    ///
    /// Used when [`super::config::PackageType`] is [`super::config::PackageType::Bin`].
    #[must_use]
    pub fn bin_path(&self, name: &str) -> PathBuf {
        self.build_root.join("bin").join(format!("{name}.phx0"))
    }

    /// Linked library output: `build/lib/{name}.phx0`.
    ///
    /// Used when [`super::config::PackageType`] is [`super::config::PackageType::Lib`].
    #[must_use]
    pub fn lib_path(&self, name: &str) -> PathBuf {
        self.build_root.join("lib").join(format!("{name}.phx0"))
    }

    /// Per-module `.pxi` and `.phx0` paths for a logical module path.
    ///
    /// `logical_path` uses `::` segment separators (e.g. `myapp::util::math`). On-disk paths
    /// mirror segments as `/` under `pxi/` and `phx0/` with `.pxi` / `.phx0` extensions.
    ///
    /// Does not inspect path dependencies; use [`Self::module_artifacts_resolved`] when the
    /// first segment may name a dependency crate.
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

    /// Per-module artifact paths with path-dependency root selection.
    ///
    /// When the first `::` segment of `logical_path` is non-empty, differs from
    /// `workspace_package`, and appears in `dep_names`, artifact paths are computed under
    /// `build/deps/{first}/` instead of the workspace root. Otherwise delegates to
    /// [`Self::module_artifacts`].
    ///
    /// Example: for workspace package `"app"`, dependency `"math"`, and logical path
    /// `"math::util"`, returns paths under `build/deps/math/pxi/math/util.pxi`.
    ///
    /// `dep_names` should list keys from the workspace `[dependencies]` table (path deps only).
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

    /// Ensures workspace build subdirectories exist before emission.
    ///
    /// Creates `pxi/`, `phx0/`, and `deps/` always. Also creates `bin/` or `lib/` depending on
    /// `package_type`. Does not create `manifest.json`; the build driver writes that after a
    /// successful compile.
    ///
    /// # Errors
    ///
    /// Returns I/O errors from `std::fs::create_dir_all` when a directory cannot be created.
    ///
    /// # Panics
    ///
    /// Never panics.
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

    /// Ensures path-dependency artifact directories exist.
    ///
    /// Creates `pxi/`, `phx0/`, and `lib/` under this layout's [`Self::build_root`]
    /// (typically `build/deps/{name}/`). Dependency builds do not use `bin/`.
    ///
    /// # Errors
    ///
    /// Returns I/O errors from `std::fs::create_dir_all` when a directory cannot be created.
    ///
    /// # Panics
    ///
    /// Never panics.
    pub fn ensure_dep_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.build_root.join("pxi"))?;
        std::fs::create_dir_all(self.build_root.join("phx0"))?;
        std::fs::create_dir_all(self.build_root.join("lib"))?;
        Ok(())
    }
}

/// Converts `a::b::c` to relative path components `a/b/c`.
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

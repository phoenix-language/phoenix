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
    /// Creates layout for `config.build_root()`.
    #[must_use]
    pub fn new(config: &ProjectConfig) -> Self {
        Self {
            build_root: config.build_root(),
        }
    }

    /// `build/manifest.json`
    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.build_root.join("manifest.json")
    }

    /// `build/bin/<bin_name>.phx0`
    #[must_use]
    pub fn bin_path(&self, bin_name: &str) -> PathBuf {
        self.build_root.join("bin").join(format!("{bin_name}.phx0"))
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

    /// Ensures `build/pxi`, `build/phx0`, and `build/bin` exist.
    ///
    /// # Errors
    ///
    /// Returns I/O errors from `create_dir_all`.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.build_root.join("pxi"))?;
        std::fs::create_dir_all(self.build_root.join("phx0"))?;
        std::fs::create_dir_all(self.build_root.join("bin"))?;
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
        let a = layout.module_artifacts("util::math");
        assert!(a.pxi.ends_with("util/math.pxi"));
        assert!(a.phx0.ends_with("util/math.phx0"));
    }
}

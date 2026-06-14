//! I/O helpers and module-path utilities for the build driver.

use std::path::{Path, PathBuf};

use crate::modules::ModulePath;
use crate::project::ProjectConfig;

use super::super::error::BuildError;

/// Maps the entry source file to its logical module path.
pub(super) fn entry_logical_path(
    config: &ProjectConfig,
    entry_file: &Path,
) -> Result<String, BuildError> {
    ModulePath::from_file_path(&config.module_root(), entry_file, &config.name)
        .map(|p| p.display())
        .ok_or_else(|| {
            BuildError::Project(crate::project::ProjectError::Invalid {
                message: "could not derive entry module path from file".to_owned(),
            })
        })
}

/// Returns true when `logical` belongs to the workspace package `workspace`.
pub(super) fn module_in_workspace_package(logical: &str, workspace: &str) -> bool {
    logical.split("::").next() == Some(workspace)
}

/// Maps a bare I/O error to [`BuildError::Io`] without a path.
pub(super) fn io_err(e: &std::io::Error) -> BuildError {
    BuildError::Io {
        path: PathBuf::new(),
        message: e.to_string(),
    }
}

/// Maps an I/O error at `path` to [`BuildError::Io`].
pub(super) fn io_err_path(path: &Path, e: &std::io::Error) -> BuildError {
    BuildError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    }
}

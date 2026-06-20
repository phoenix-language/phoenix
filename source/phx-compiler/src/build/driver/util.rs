//! I/O helpers and module-path utilities for the build driver (M2).
//!
//! Shared by [`super::package`], [`super::incremental`], and [`super::artifacts`]:
//!
//! - [`entry_logical_path`] — derive the entry module's logical path from its source file
//! - [`module_in_workspace_package`] — filter modules owned by the workspace crate
//! - [`io_err`] / [`io_err_path`] — normalize [`std::io::Error`] into [`BuildError::Io`]
//!
//! These functions do not perform compilation; they translate project layout and filesystem
//! failures into the driver's single error type.

use std::path::{Path, PathBuf};

use crate::modules::ModulePath;
use crate::project::ProjectConfig;

use super::super::error::BuildError;

/// Maps the entry source file to its logical module path.
///
/// Uses [`ModulePath::from_file_path`] with the project's [`ProjectConfig::module_root`]
/// and package name. The result is the dotted path stored in build artifacts and link maps
/// (e.g. `my_crate` for `src/main.phx` in a binary crate).
///
/// # Errors
///
/// Returns [`BuildError::Project`] when the entry file lies outside the module root or
/// cannot be expressed as a valid logical module path.
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

/// Returns `true` when `logical` belongs to the workspace package `workspace`.
///
/// Compares the first segment of the dotted logical path to `workspace`. Used by incremental
/// freshness checks and artifact emission to include only modules defined in the current
/// crate, excluding path-dependency modules that appear in the loaded program graph.
pub(super) fn module_in_workspace_package(logical: &str, workspace: &str) -> bool {
    logical.split("::").next() == Some(workspace)
}

/// Maps a bare I/O error to [`BuildError::Io`] without a path.
///
/// Use when the failing operation has no associated filesystem path (e.g. creating a temp
/// directory handle). The resulting error has an empty [`BuildError::Io`] path field.
pub(super) fn io_err(e: &std::io::Error) -> BuildError {
    BuildError::Io {
        path: PathBuf::new(),
        message: e.to_string(),
    }
}

/// Maps an I/O error at `path` to [`BuildError::Io`].
///
/// Preferred over [`io_err`] when the caller knows which file or directory operation
/// failed; the path is included in [`BuildError::to_message`] output.
pub(super) fn io_err_path(path: &Path, e: &std::io::Error) -> BuildError {
    BuildError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    }
}

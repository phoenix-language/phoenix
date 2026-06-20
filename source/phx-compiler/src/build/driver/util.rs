//! I/O helpers and module-path utilities for the build driver (M2).
//!
//! Shared by [`super::package`], [`super::incremental`], [`super::artifacts`], and
//! [`super::link_map`]. These helpers translate project layout conventions and filesystem
//! failures into [`BuildError`] without performing compilation themselves.
//!
//! ## Role in the build driver
//!
//! ```text
//! ProjectConfig + entry path → entry_logical_path → logical module id (link map key)
//! LoadedProgram.modules      → module_in_workspace_package → workspace-only filter
//! std::fs::* failures        → io_err / io_err_path → BuildError::Io
//! ```
//!
//! [`entry_logical_path`] is the bridge between on-disk entry files (`src/main.phx`,
//! `src/lib.phx`, or a user override) and the dotted logical paths stored in build
//! manifests, `.pxi` files, and link inputs. [`module_in_workspace_package`] separates
//! modules defined in the current crate from path-dependency modules that appear in the
//! same [`LoadedProgram`](crate::modules::LoadedProgram) graph after `load_program`.
//!
//! ## Callers
//!
//! | Function | Used by |
//! | --- | --- |
//! | [`entry_logical_path`] | [`super::package`] — `build_package`, `load_project_binary*` |
//! | [`module_in_workspace_package`] | [`super::incremental`], [`super::artifacts`], [`super::link_map`] |
//! | [`io_err`] / [`io_err_path`] | [`super::package`], [`super::artifacts`] — read/write linked output |

use std::path::{Path, PathBuf};

use crate::modules::ModulePath;
use crate::project::ProjectConfig;

use super::super::error::BuildError;

/// Maps the entry source file to its logical module path string.
///
/// Delegates to [`ModulePath::from_file_path`] with [`ProjectConfig::module_root`] and
/// [`ProjectConfig::name`]. The returned dotted path is the key used in build manifests,
/// `.pxi` `logical_module` fields, and the link map entry for the workspace crate.
///
/// ## Examples
///
/// | Entry file (under `module_src`) | Package `name` | Logical path |
/// | --- | --- | --- |
/// | `src/main.phx` | `demo` | `demo` |
/// | `src/lib.phx` | `mylib` | `mylib` |
/// | `src/utils/mod.phx` | `demo` | `demo::utils` |
///
/// Root entry files (`main.phx` / `lib.phx`) collapse to the package name alone; nested
/// modules include intermediate directory segments before the final stem (unless the stem
/// is `mod`, in which case the directory path is used).
///
/// # Errors
///
/// Returns [`BuildError::Project`] with [`crate::project::ProjectError::Invalid`] when
/// `entry_file` is not under `config.module_root()`, has no `.phx` suffix, or cannot be
/// expressed as a valid [`ModulePath`].
///
/// # Panics
///
/// Never panics on user-supplied paths.
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
/// Compares the first `::`-separated segment of `logical` to `workspace`. After
/// [`load_program_with_context`](crate::modules::load_program_with_context), the
/// [`LoadedProgram`](crate::modules::LoadedProgram) contains modules from path dependencies
/// as well as the workspace crate; this filter keeps incremental staleness checks,
/// `.pxi`/`.phx0` emission, and link-map construction scoped to the crate being built.
///
/// A dependency module `math::ops` in a workspace named `app` returns `false` when
/// `workspace` is `"app"`. The workspace root module `app` and nested `app::utils` return
/// `true`.
///
/// # Panics
///
/// Never panics; an empty `logical` string compares unequal to any non-empty `workspace`.
pub(super) fn module_in_workspace_package(logical: &str, workspace: &str) -> bool {
    logical.split("::").next() == Some(workspace)
}

/// Maps a bare I/O error to [`BuildError::Io`] without a path.
///
/// Prefer [`io_err_path`] when the caller knows which file or directory failed; use this
/// helper only when no path is meaningful (e.g. a temp-file handle created in memory).
/// The resulting [`BuildError::Io`] carries an empty `path` field, so
/// [`BuildError::to_message`] omits a location prefix.
///
/// # Panics
///
/// Never panics.
pub(super) fn io_err(e: &std::io::Error) -> BuildError {
    BuildError::Io {
        path: PathBuf::new(),
        message: e.to_string(),
    }
}

/// Maps an I/O error at `path` to [`BuildError::Io`].
///
/// Preferred over [`io_err`] for read, write, and `create_dir_all` failures on build
/// artifacts (linked `.phx0`, per-module `.pxi`, manifest JSON). The path is copied into
/// the error and surfaced in [`BuildError::to_message`] for CLI diagnostics.
///
/// # Panics
///
/// Never panics.
pub(super) fn io_err_path(path: &Path, e: &std::io::Error) -> BuildError {
    BuildError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    }
}

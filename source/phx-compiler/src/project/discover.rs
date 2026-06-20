//! Project discovery — locate `phoenix.toml` by walking parent directories (M2).
//!
//! When the CLI receives a source file or cwd without an explicit `--project` root,
//! [`discover_project`] walks upward from the starting path until it finds
//! `phoenix.toml`, then delegates to [`ProjectConfig::load`].
//!
//! [`resolve_project`] chooses between an explicit root and discovery.
//!
//! ## CLI integration
//!
//! `phx check`, `phx build`, and `phx run` call [`resolve_project`] with the entry path
//! and an optional `--project` override. When no override is given, [`discover_project`]
//! walks from the entry file's directory (or the cwd when the path is a directory) toward
//! the filesystem root.
//!
//! ## Discovery rules
//!
//! - **Files vs directories:** a file path starts discovery in its parent; a directory starts
//!   in that directory.
//! - **Marker:** the first ancestor directory containing `phoenix.toml` wins; child directories
//!   are not searched when walking upward.
//! - **Failure:** [`ProjectError::NotFound`] includes the original `start` path for diagnostics.

use std::path::Path;

use super::config::{ProjectConfig, ProjectError};

/// Finds the nearest `phoenix.toml` starting at `start` and walking upward.
///
/// When `start` is a file path, discovery begins at its parent directory (or `"."` when the
/// file has no parent). Each ancestor is tested for `phoenix.toml`; the first hit is loaded
/// via [`ProjectConfig::load`]. Stops at the filesystem root and returns
/// [`ProjectError::NotFound`] if no marker exists.
///
/// # Errors
///
/// Returns [`ProjectError::NotFound`] when no marker file exists in any ancestor directory.
/// Returns other [`ProjectError`] variants from [`ProjectConfig::load`] when the marker is
/// present but invalid (parse failure, missing entry file, bad dependency path, etc.).
///
/// # Panics
///
/// Never panics on user-supplied paths; a file with no parent falls back to `"."`.
pub fn discover_project(start: &Path) -> Result<ProjectConfig, ProjectError> {
    let mut dir = if start.is_file() {
        start.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        let marker = dir.join("phoenix.toml");
        if marker.is_file() {
            return ProjectConfig::load(&dir);
        }
        if !dir.pop() {
            break;
        }
    }
    Err(ProjectError::NotFound {
        from: start.to_path_buf(),
    })
}

/// Resolves project config from explicit root or discovery.
///
/// When `project_root` is `Some`, loads that directory directly (equivalent to passing
/// `--project` on the CLI). When `None`, delegates to [`discover_project`] starting at `start`.
///
/// # Errors
///
/// Returns [`ProjectError::NotFound`] when discovery fails to locate `phoenix.toml`.
/// Returns other [`ProjectError`] variants when configuration cannot be loaded or validated.
pub fn resolve_project(
    start: &Path,
    project_root: Option<&Path>,
) -> Result<ProjectConfig, ProjectError> {
    if let Some(root) = project_root {
        return ProjectConfig::load(root);
    }
    discover_project(start)
}

//! Project discovery — locate `phoenix.toml` by walking parent directories (M2).
//!
//! When the CLI receives a source file or cwd without an explicit `--project` root,
//! [`discover_project`] walks upward from the starting path until it finds
//! `phoenix.toml`, then delegates to [`ProjectConfig::load`].
//!
//! [`resolve_project`] chooses between an explicit root and discovery.

use std::path::Path;

use super::config::{ProjectConfig, ProjectError};

/// Finds the nearest `phoenix.toml` starting at `start` and walking upward.
///
/// When `start` is a file path, discovery begins at its parent directory. Stops at the
/// filesystem root and returns [`ProjectError::NotFound`] if no marker exists.
///
/// # Errors
///
/// Returns [`ProjectError::NotFound`] when no marker file exists, or other
/// [`ProjectError`] variants from [`ProjectConfig::load`].
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
/// When `project_root` is `Some`, loads that directory directly; otherwise delegates to
/// [`discover_project`].
///
/// # Errors
///
/// Returns [`ProjectError`] when configuration cannot be loaded.
pub fn resolve_project(
    start: &Path,
    project_root: Option<&Path>,
) -> Result<ProjectConfig, ProjectError> {
    if let Some(root) = project_root {
        return ProjectConfig::load(root);
    }
    discover_project(start)
}

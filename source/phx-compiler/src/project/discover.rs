//! Locate `phoenix.toml` by walking parent directories.

use std::path::Path;

use super::config::{ProjectConfig, ProjectError};

/// Finds the nearest `phoenix.toml` starting at `start` and walking upward.
///
/// # Errors
///
/// Returns [`ProjectError::NotFound`] when no marker file exists.
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

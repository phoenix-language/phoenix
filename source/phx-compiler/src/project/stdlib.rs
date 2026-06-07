//! Locating the bundled Phoenix standard library (`std` package).

use std::path::{Path, PathBuf};

use super::config::{PathDependency, ProjectConfig, ProjectError};

/// Environment variable overriding the bundled `std` package root.
pub const PHOENIX_STD_ENV: &str = "PHOENIX_STD";

/// Resolves the filesystem root of the bundled `std` library package.
///
/// Search order:
/// 1. [`PHOENIX_STD_ENV`] when set and contains a valid `std` `phoenix.toml`
/// 2. Walk parents of `std::env::current_exe()` for `std/phoenix.toml` with `project.name = "std"`
/// 3. Walk parents of the process current directory (same rule)
///
/// # Errors
///
/// Returns [`ProjectError::Invalid`] when no bundled std root can be found.
pub fn resolve_bundled_std_root() -> Result<PathBuf, ProjectError> {
    if let Ok(env) = std::env::var(PHOENIX_STD_ENV) {
        let root = PathBuf::from(env.trim());
        return validate_std_root(&root).map(|()| root.canonicalize().unwrap_or(root));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(root) = find_std_in_ancestors(exe.parent())
    {
        return Ok(root);
    }
    if let Ok(cwd) = std::env::current_dir()
        && let Some(root) = find_std_in_ancestors(Some(&cwd))
    {
        return Ok(root);
    }
    Err(ProjectError::Invalid {
        message: format!(
            "bundled std package not found; set {PHOENIX_STD_ENV} to the std project root \
             or add `[dependencies] std = {{ path = ... }}` with bundle_std = false"
        ),
    })
}

/// Injects the bundled `std` path dependency when [`ProjectConfig::bundle_std`] is enabled.
///
/// # Errors
///
/// Returns [`ProjectError::Invalid`] when bundling is required but std cannot be located.
pub fn apply_bundled_std(config: &mut ProjectConfig) -> Result<(), ProjectError> {
    if !config.bundle_std || config.name == "std" || config.dependencies.contains_key("std") {
        return Ok(());
    }
    let abs = resolve_bundled_std_root()?;
    let path = dependency_path_for_root(&config.root, &abs);
    config
        .dependencies
        .insert("std".to_owned(), PathDependency { path });
    Ok(())
}

/// Path to record in dependency tables (relative to `project_root` when possible).
#[must_use]
pub fn dependency_path_for_root(project_root: &Path, std_root: &Path) -> PathBuf {
    let project = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let std_abs = std_root
        .canonicalize()
        .unwrap_or_else(|_| std_root.to_path_buf());
    std_abs
        .strip_prefix(&project)
        .map(Path::to_path_buf)
        .unwrap_or(std_abs)
}

fn find_std_in_ancestors(mut dir: Option<&Path>) -> Option<PathBuf> {
    while let Some(d) = dir {
        let candidate = d.join("std");
        if validate_std_root(&candidate).is_ok() {
            return Some(candidate.canonicalize().unwrap_or(candidate));
        }
        dir = d.parent();
    }
    None
}

fn validate_std_root(root: &Path) -> Result<(), ProjectError> {
    let manifest = root.join("phoenix.toml");
    if !manifest.is_file() {
        return Err(ProjectError::Invalid {
            message: format!("missing phoenix.toml at {}", manifest.display()),
        });
    }
    let cfg = ProjectConfig::load_without_bundled_std(root)?;
    if cfg.name != "std" {
        return Err(ProjectError::Invalid {
            message: format!("expected project.name `std`, found `{}`", cfg.name),
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn std_smoke_fixture_bundles_std() {
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/std_smoke");
        if !root.join("phoenix.toml").is_file() {
            return;
        }
        let cfg = ProjectConfig::load(&root).expect("load std_smoke");
        assert!(cfg.bundle_std);
        let dep = cfg.dependencies.get("std").expect("std");
        let dep_root = root.join(&dep.path);
        assert!(
            dep_root.join("phoenix.toml").is_file(),
            "std dep root {} (path {:?})",
            dep_root.display(),
            dep.path
        );
    }

    #[test]
    fn resolves_repo_std_via_env() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../std/phoenix.toml");
        if !manifest.is_file() {
            return;
        }
        let std_root = manifest.parent().expect("std root");
        // SAFETY: test-only env mutation; single-threaded cargo test harness.
        unsafe { std::env::set_var(PHOENIX_STD_ENV, std_root) };
        let resolved = resolve_bundled_std_root().expect("bundled std");
        assert_eq!(
            resolved.canonicalize().unwrap_or(resolved),
            std_root
                .canonicalize()
                .unwrap_or_else(|_| std_root.to_path_buf())
        );
    }
}

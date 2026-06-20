//! Bundled standard library (`std` package) discovery and dependency injection (M2).
//!
//! Phoenix ships a compiler-bundled `std` library package. When [`ProjectConfig::bundle_std`] is
//! true (the default), [`ProjectConfig::load`] calls [`apply_bundled_std`] to locate that package
//! on disk and insert a `path` dependency keyed `"std"` into [`ProjectConfig::dependencies`].
//!
//! ## Inputs and outputs
//!
//! - **Input:** Process environment ([`PHOENIX_STD_ENV`]), executable location, and current
//!   working directory; an in-memory [`ProjectConfig`] during load.
//! - **Output:** Absolute or project-relative path to the `std` package root; a mutated
//!   [`ProjectConfig::dependencies`] entry `{ path = ... }` when bundling applies.
//!
//! ## Search order
//!
//! [`resolve_bundled_std_root`] tries, in order:
//!
//! 1. [`PHOENIX_STD_ENV`] when set and pointing at a directory containing `phoenix.toml` with
//!    `project.name = "std"`.
//! 2. Ancestors of `std::env::current_exe()` containing a `std/phoenix.toml` sibling directory
//!    (typical when the `phx` binary lives under a Phoenix checkout or install prefix).
//! 3. Ancestors of the process current directory (same `std/phoenix.toml` rule).
//!
//! ## When bundling is skipped
//!
//! [`apply_bundled_std`] is a no-op when any of the following hold:
//!
//! - [`ProjectConfig::bundle_std`] is `false` (user declared `std` manually or opts out).
//! - [`ProjectConfig::name`] is `"std"` (loading the std package itself — avoids recursion).
//! - `dependencies` already contains a `"std"` key (explicit `[dependencies]` entry wins).
//!
//! ## Entry points
//!
//! - [`resolve_bundled_std_root`] — locate the `std` package root on disk
//! - [`apply_bundled_std`] — mutate a [`ProjectConfig`] to add the bundled dependency
//! - [`dependency_path_for_root`] — prefer project-relative paths in `phoenix.toml`
//!
//! ## Related APIs
//!
//! - [`ProjectConfig::load`] — primary caller; invokes [`apply_bundled_std`] after parse
//! - [`ProjectConfig::load_without_bundled_std`] — skips injection when validating the std root

use std::path::{Path, PathBuf};

use super::config::{PathDependency, ProjectConfig, ProjectError};

/// Environment variable overriding the bundled `std` package root.
///
/// When set, [`resolve_bundled_std_root`] reads this value first (after trimming whitespace)
/// and validates that the path contains a `phoenix.toml` with `project.name = "std"`.
///
/// Useful for development, CI, and embedders that install `std` outside the compiler prefix:
///
/// ```ignore
/// export PHOENIX_STD=/path/to/phoenix/std
/// phx build
/// ```
pub const PHOENIX_STD_ENV: &str = "PHOENIX_STD";

/// Resolves the filesystem root of the bundled `std` library package.
///
/// Returns the canonicalized directory containing the std `phoenix.toml`. Search order matches
/// the module-level list: env override, then exe ancestors, then cwd ancestors.
///
/// Each candidate directory must contain `phoenix.toml` and parse as a project named `"std"`.
/// Validation uses [`ProjectConfig::load_without_bundled_std`] so the std package can be
/// checked without re-entering bundled-std injection.
///
/// # Errors
///
/// Returns [`ProjectError::Invalid`] when:
///
/// - No candidate root is found after exhausting all search paths.
/// - [`PHOENIX_STD_ENV`] points at a path missing `phoenix.toml` or with `project.name != "std"`.
/// - A discovered `std/phoenix.toml` fails to parse or validate.
///
/// The error message suggests setting [`PHOENIX_STD_ENV`] or declaring
/// `[dependencies] std = { path = ... }` with `bundle_std = false`.
///
/// # Panics
///
/// Never panics on missing or malformed filesystem state; failures surface as
/// [`ProjectError::Invalid`].
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
/// When bundling applies, resolves the std root via [`resolve_bundled_std_root`], converts it
/// to a path relative to [`ProjectConfig::root`] when possible via [`dependency_path_for_root`],
/// and inserts `dependencies["std"] = { path = ... }`.
///
/// Does not call [`ProjectConfig::validate`]; callers ([`ProjectConfig::load`]) validate after
/// injection.
///
/// # Errors
///
/// Returns [`ProjectError::Invalid`] from [`resolve_bundled_std_root`] when bundling is required
/// but the std package cannot be located or validated.
///
/// Returns `Ok(())` without mutation when bundling is skipped (see module-level "When bundling
/// is skipped").
///
/// # Panics
///
/// Never panics.
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

/// Path to record in `[dependencies]` tables (relative to `project_root` when possible).
///
/// Canonicalizes both `project_root` and `std_root` when the OS allows, then returns
/// `std_root` stripped of the `project_root` prefix. When `std_root` is not under
/// `project_root` (e.g. compiler-installed std outside the workspace), returns the
/// canonical absolute `std_root` path instead.
///
/// Relative paths keep `phoenix.toml` portable across machines that share the same repo layout;
/// absolute paths are used only when no common prefix exists.
///
/// # Panics
///
/// Never panics; canonicalization failures fall back to the original path buffers.
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

/// Walks `dir` and each parent for a `std/` subdirectory that passes [`validate_std_root`].
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

/// Confirms `root/phoenix.toml` exists and parses as project name `"std"`.
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
        let root = crate::embed::project_root_path("std_smoke");
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

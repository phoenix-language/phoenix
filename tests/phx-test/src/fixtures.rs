//! Canonical paths to shared CLI test fixtures.

use std::path::{Path, PathBuf};

/// Repository root (`phoenix/`).
///
/// Resolved from this crate's manifest at `tests/phx-test/`.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// Directory containing single-file and project fixtures (`tests/cli/fixtures/`).
pub fn cli_fixtures_dir() -> PathBuf {
    repo_root().join("tests/cli/fixtures")
}

/// Path to a single-file fixture under [`cli_fixtures_dir`].
pub fn cli_fixture(name: &str) -> PathBuf {
    cli_fixtures_dir().join(name)
}

/// Path to a project fixture directory (contains `phoenix.toml`).
pub fn cli_project(name: &str) -> PathBuf {
    cli_fixtures_dir().join(name)
}

/// Module root for multi-file `#import` fixtures.
pub fn cli_modules_dir() -> PathBuf {
    cli_fixtures_dir().join("modules")
}

/// Top-level demonstration programs (`examples/`).
pub fn examples_dir() -> PathBuf {
    repo_root().join("examples")
}

/// Path to an example project directory under [`examples_dir`].
pub fn examples_project(name: &str) -> PathBuf {
    examples_dir().join(name)
}

/// Assert a fixture path exists (panics with a clear message when missing).
pub fn assert_fixture_exists(path: &Path) {
    assert!(
        path.is_file() || path.is_dir(),
        "missing fixture: {}",
        path.display()
    );
}

/// Assert a fixture file exists; returns `path` for chaining.
pub fn require_fixture_file(path: &Path) -> &Path {
    assert!(path.is_file(), "missing fixture file: {}", path.display());
    path
}

/// CLI project fixture directory; panics when `phoenix.toml` is absent.
pub fn require_cli_project(name: &str) -> PathBuf {
    let root = cli_project(name);
    require_fixture_file(&root.join("phoenix.toml"));
    root
}

/// Bundled stdlib project root (`std/phoenix.toml`).
pub fn require_std_project() -> PathBuf {
    let root = repo_root().join("std");
    require_fixture_file(&root.join("phoenix.toml"));
    root
}

/// Entry source for a CLI project fixture.
pub fn cli_project_main(name: &str) -> PathBuf {
    require_cli_project(name).join("src/main.phx")
}

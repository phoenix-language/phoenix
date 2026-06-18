//! Canonical paths and materialization for embedded test programs.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::programs::modules_main_tree;
use crate::sandbox::sandbox_project_root;
use crate::workspace::TempWorkspace;

static HELD_WORKSPACES: Mutex<Vec<TempWorkspace>> = Mutex::new(Vec::new());

/// Repository root (`phoenix/`).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// Top-level demonstration programs (`examples/`).
pub fn examples_dir() -> PathBuf {
    repo_root().join("examples")
}

/// Path to an example project directory under [`examples_dir`].
pub fn examples_project(name: &str) -> PathBuf {
    examples_dir().join(name)
}

/// Assert a path exists (panics with a clear message when missing).
pub fn assert_fixture_exists(path: &Path) {
    assert!(
        path.is_file() || path.is_dir(),
        "missing path: {}",
        path.display()
    );
}

/// Assert a file exists; returns `path` for chaining.
pub fn require_fixture_file(path: &Path) -> &Path {
    assert!(path.is_file(), "missing file: {}", path.display());
    path
}

/// Bundled stdlib project root (`std/phoenix.toml`).
pub fn require_std_project() -> PathBuf {
    let root = repo_root().join("std");
    require_fixture_file(&root.join("phoenix.toml"));
    root
}

/// Materialize a single-file program; caller must keep `TempWorkspace` alive.
pub fn materialize_single(name: &str) -> (TempWorkspace, PathBuf) {
    let program = crate::programs::lookup_single(name);
    let ws = TempWorkspace::new(name);
    let path = ws.write_single(program);
    (ws, path)
}

/// Materialize a project spec from the shared sandbox (path deps resolve as siblings).
pub fn materialize_project(name: &str) -> ((), PathBuf) {
    ((), sandbox_project_root(name))
}

/// Materialize a module tree; returns workspace, module root, and entry path.
pub fn materialize_module_tree(
    tree: &phx_programs::ModuleTree,
) -> (TempWorkspace, PathBuf, PathBuf) {
    let ws = TempWorkspace::new(tree.name);
    let entry = ws.write_module_tree(tree);
    let root = ws.root().join("tests/cli/fixtures/modules");
    (ws, root, entry)
}

/// Materialize a single-file program and pin until process exit (legacy helper).
pub fn cli_fixture(name: &str) -> PathBuf {
    let (ws, path) = materialize_single(name);
    HELD_WORKSPACES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(ws);
    path
}

/// Legacy: module root for multi-file `#import` fixtures (`modules/main.phx` tree).
pub fn cli_modules_dir() -> PathBuf {
    let (ws, root, _) = materialize_module_tree(modules_main_tree());
    HELD_WORKSPACES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(ws);
    root
}

/// Project root under the shared fixture sandbox (legacy helper).
pub fn cli_project(name: &str) -> PathBuf {
    sandbox_project_root(name)
}

/// CLI project fixture directory; panics when materialization fails.
pub fn require_cli_project(name: &str) -> PathBuf {
    let root = cli_project(name);
    require_fixture_file(&root.join("phoenix.toml"));
    root
}

/// Entry source for a materialized CLI project fixture.
pub fn cli_project_main(name: &str) -> PathBuf {
    require_cli_project(name).join("src/main.phx")
}

/// Deprecated: fixtures are embedded in `phx-programs`.
pub fn cli_fixtures_dir() -> PathBuf {
    cli_modules_dir()
}

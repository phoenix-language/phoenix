//! Local test helpers for phx-compiler pass tests.

#![allow(dead_code)]

use std::path::PathBuf;

use phx_compiler::{CompileError, compile_source};
use phx_diagnostics::{DiagnosticBag, TypeCheckBag};
use phx_programs::ModuleTree;

/// Source text for a single-file embedded program.
pub fn single_source(name: &str) -> &'static str {
    phx_programs::single::single_by_name(name)
        .unwrap_or_else(|| panic!("unknown single-file program: {name}"))
        .source
}

/// Main source for an embedded project.
pub fn project_main_source(name: &str) -> &'static str {
    let spec = phx_programs::projects::project_by_name(name)
        .unwrap_or_else(|| panic!("unknown project: {name}"));
    spec.files
        .iter()
        .find(|(path, _)| *path == "src/main.phx")
        .map_or_else(
            || panic!("project {name} has no src/main.phx"),
            |(_, src)| *src,
        )
}

/// Path to a materialized single-file program for `check_file`.
pub fn single_file_path(name: &str) -> PathBuf {
    let program = phx_programs::single::single_by_name(name)
        .unwrap_or_else(|| panic!("unknown program: {name}"));
    let root = temp_root(program.name);
    let logical = format!("tests/cli/fixtures/{}", program.name);
    write_at(&root, &logical, program.source);
    root.join(logical)
}

/// Path to a materialized project main for `check_file`.
pub fn embedded_project_main(name: &str) -> PathBuf {
    project_root_path(name).join("src/main.phx")
}

fn project_root_path(name: &str) -> PathBuf {
    let spec = phx_programs::projects::project_by_name(name)
        .unwrap_or_else(|| panic!("unknown project: {name}"));
    let root = temp_root(spec.name);
    let base = format!("tests/cli/fixtures/{}", spec.name);
    write_at(&root, &format!("{base}/phoenix.toml"), spec.toml);
    for (rel, source) in spec.files {
        if !rel.ends_with(".gitkeep") {
            write_at(&root, &format!("{base}/{rel}"), source);
        }
    }
    root.join(base)
}

fn temp_root(label: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "phx_compiler_test_path_{label}_{}_{n}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("mkdir temp root");
    root
}

fn write_at(root: &std::path::Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&path, contents).expect("write embedded source");
}

/// Path to materialized project main for lint tests that need a file path.
pub fn project_main_path(name: &str) -> PathBuf {
    embedded_project_main(name)
}

/// Expect `compile_source` to succeed.
pub fn compile_ok(source: &str) {
    compile_source(source, None).unwrap_or_else(|e| panic!("expected ok: {e}"));
}

/// Expect `compile_source` to fail; run `f` on the error.
pub fn expect_compile_err(source: &str, f: impl FnOnce(CompileError)) {
    match compile_source(source, None) {
        Err(err) => f(err),
        Ok(_) => panic!("expected compile error"),
    }
}

/// Expect a resolve-phase failure and return the diagnostic bag.
pub fn expect_resolve_err(source: &str) -> DiagnosticBag {
    match compile_source(source, None) {
        Err(CompileError::Resolve { bag, .. }) => bag,
        Err(other) => panic!("expected resolve error, got {other}"),
        Ok(_) => panic!("expected resolve error"),
    }
}

/// Expect a type-check failure and return the diagnostic bag.
pub fn expect_typeck_err(source: &str) -> TypeCheckBag {
    match compile_source(source, None) {
        Err(CompileError::TypeCheck { bag, .. }) => bag,
        Err(other) => panic!("expected type-check error, got {other}"),
        Ok(_) => panic!("expected type-check error"),
    }
}

/// Write a module tree to a temp directory; returns `(module_root, entry_path)`.
pub fn materialize_module_tree(tree: &ModuleTree) -> (PathBuf, PathBuf) {
    let n = std::process::id();
    let root = std::env::temp_dir().join(format!("phx_compiler_test_{}_{}", tree.name, n));
    let _ = std::fs::remove_dir_all(&root);
    let module_root = root.join("tests/cli/fixtures/modules");
    for (rel, source) in tree.files {
        let path = module_root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(&path, source).expect("write module file");
    }
    let entry = module_root.join(tree.entry);
    (module_root, entry)
}

/// Entry source for a named module tree.
pub fn module_entry_source(tree_name: &str) -> (&'static str, &'static str) {
    let tree = phx_programs::modules::module_tree_by_name(tree_name)
        .unwrap_or_else(|| panic!("unknown module tree: {tree_name}"));
    let source = tree
        .files
        .iter()
        .find(|(path, _)| *path == tree.entry)
        .map_or_else(
            || panic!("missing entry {} in tree {tree_name}", tree.entry),
            |(_, src)| *src,
        );
    (tree.entry, source)
}
/// Module tree root and entry path for resolve integration tests.
pub fn modules_fixture_root_and_entry(tree_name: &str) -> (PathBuf, PathBuf) {
    let tree = phx_programs::modules::module_tree_by_name(tree_name)
        .unwrap_or_else(|| panic!("unknown module tree: {tree_name}"));
    materialize_module_tree(tree)
}

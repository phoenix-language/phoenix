//! Local test helpers for phx-compiler pass tests.

#![allow(
    dead_code,
    clippy::expect_used,
    clippy::explicit_auto_deref,
    clippy::manual_let_else,
    clippy::match_wild_err_arm,
    clippy::missing_panics_doc,
    clippy::unwrap_used
)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static MODULE_TREE_MATERIALIZE_COUNTER: AtomicU64 = AtomicU64::new(0);

use phx_bytecode::Instruction;
use phx_compiler::{CompileError, compile_source};
use phx_diagnostics::{DiagnosticBag, TypeCheckBag};
use phx_programs::ModuleTree;

/// Unwrap a test [`Result`], panicking with `context` on failure.
pub fn test_ok<T, E: std::fmt::Debug>(result: Result<T, E>, context: &str) -> T {
    match result {
        Ok(value) => value,
        Err(err) => panic!("{context}: {err:?}"),
    }
}

/// Expect a test [`Result`] to be `Err`, returning the error value.
#[must_use]
pub fn test_err<T, E: std::fmt::Debug>(result: Result<T, E>, context: &str) -> E {
    match result {
        Err(err) => err,
        Ok(_) => panic!("{context}: expected error"),
    }
}

/// Unwrap a test [`Option`], panicking with `context` if absent.
#[must_use]
pub fn test_some<T>(option: Option<T>, context: &str) -> T {
    match option {
        Some(value) => value,
        None => panic!("{context}: expected Some"),
    }
}

/// Find a substring in `haystack`, panicking if absent.
#[must_use]
pub fn test_find(haystack: &str, needle: &str, context: &str) -> usize {
    match haystack.find(needle) {
        Some(offset) => offset,
        None => panic!("{context}: {needle:?} not found"),
    }
}

/// Reverse-find a substring in `haystack`, panicking if absent.
#[must_use]
pub fn test_rfind(haystack: &str, needle: &str, context: &str) -> usize {
    match haystack.rfind(needle) {
        Some(offset) => offset,
        None => panic!("{context}: {needle:?} not found"),
    }
}

/// Convert `usize` to `u32` for test offsets, panicking on overflow.
#[must_use]
pub fn u32_from_usize(value: usize, context: &str) -> u32 {
    match u32::try_from(value) {
        Ok(offset) => offset,
        Err(err) => panic!("{context}: {value} exceeds u32::MAX: {err}"),
    }
}

/// Copy a byte slice into a fixed-size array in tests.
#[must_use]
pub fn test_array<const N: usize>(slice: &[u8], context: &str) -> [u8; N] {
    match slice.try_into() {
        Ok(array) => array,
        Err(_) => panic!("{context}: expected slice length {N}, got {}", slice.len()),
    }
}

/// Encode a bytecode instruction in tests.
#[must_use]
pub fn test_encode(inst: &Instruction) -> Vec<u8> {
    match inst.encode() {
        Ok(bytes) => bytes,
        Err(err) => panic!("encode {inst:?}: {err:?}"),
    }
}

/// First item from an iterator, panicking if empty.
#[must_use]
pub fn test_first<I: Iterator>(iter: &mut I, context: &str) -> I::Item {
    match iter.next() {
        Some(item) => item,
        None => panic!("{context}: expected non-empty iterator"),
    }
}

/// Create a directory and all parents in tests.
pub fn fs_create_dir_all(path: &Path, context: &str) {
    if let Err(err) = std::fs::create_dir_all(path) {
        panic!("{context}: {err}");
    }
}

/// Write file contents in tests.
pub fn fs_write(path: &Path, contents: impl AsRef<[u8]>, context: &str) {
    if let Err(err) = std::fs::write(path, contents) {
        panic!("{context}: {err}");
    }
}

/// Source text for a single-file embedded program.
pub fn single_source(name: &str) -> &'static str {
    match phx_programs::single::single_by_name(name) {
        Some(program) => program.source,
        None => panic!("unknown single-file program: {name}"),
    }
}

/// Main source for an embedded project.
pub fn project_main_source(name: &str) -> &'static str {
    let spec = match phx_programs::projects::project_by_name(name) {
        Some(spec) => spec,
        None => panic!("unknown project: {name}"),
    };
    match spec.files.iter().find(|(path, _)| *path == "src/main.phx") {
        Some((_, src)) => *src,
        None => panic!("project {name} has no src/main.phx"),
    }
}

/// Path to a materialized single-file program for `check_file`.
pub fn single_file_path(name: &str) -> PathBuf {
    let program = match phx_programs::single::single_by_name(name) {
        Some(program) => program,
        None => panic!("unknown program: {name}"),
    };
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
    let spec = match phx_programs::projects::project_by_name(name) {
        Some(spec) => spec,
        None => panic!("unknown project: {name}"),
    };
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
    fs_create_dir_all(&root, "mkdir temp root");
    root
}

fn write_at(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs_create_dir_all(parent, "mkdir");
    }
    fs_write(&path, contents, "write embedded source");
}

/// Path to materialized project main for lint tests that need a file path.
pub fn project_main_path(name: &str) -> PathBuf {
    embedded_project_main(name)
}

/// Expect `compile_source` to succeed.
pub fn compile_ok(source: &str) {
    match compile_source(source, None) {
        Ok(_) => {}
        Err(err) => panic!("expected ok: {err}"),
    }
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
    let unique = MODULE_TREE_MATERIALIZE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("phx_compiler_test_{}_{}_{unique}", tree.name, n));
    let _ = std::fs::remove_dir_all(&root);
    let module_root = root.join("tests/cli/fixtures/modules");
    for (rel, source) in tree.files {
        let path = module_root.join(rel);
        if let Some(parent) = path.parent() {
            fs_create_dir_all(parent, "mkdir");
        }
        fs_write(&path, source, "write module file");
    }
    let entry = module_root.join(tree.entry);
    (module_root, entry)
}

/// Entry source for a named module tree.
pub fn module_entry_source(tree_name: &str) -> (&'static str, &'static str) {
    let tree = match phx_programs::modules::module_tree_by_name(tree_name) {
        Some(tree) => tree,
        None => panic!("unknown module tree: {tree_name}"),
    };
    let source = match tree.files.iter().find(|(path, _)| *path == tree.entry) {
        Some((_, src)) => *src,
        None => panic!("missing entry {} in tree {tree_name}", tree.entry),
    };
    (tree.entry, source)
}

/// Module tree root and entry path for resolve integration tests.
pub fn modules_fixture_root_and_entry(tree_name: &str) -> (PathBuf, PathBuf) {
    let tree = match phx_programs::modules::module_tree_by_name(tree_name) {
        Some(tree) => tree,
        None => panic!("unknown module tree: {tree_name}"),
    };
    materialize_module_tree(tree)
}

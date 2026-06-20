//! Materialize embedded Phoenix programs from [`phx_programs`] to on-disk temp paths.
//!
//! In-crate unit tests call [`single_file_path`], [`project_root_path`], or [`project_main_path`]
//! to obtain real filesystem paths for [`crate::check_file`], [`crate::compile_to_module`], and
//! other path-based drivers — without copying fixture sources into the repo tree.
//!
//! ## Relationship to other test helpers
//!
//! | Helper | Location | Use when |
//! | --- | --- | --- |
//! | This module | `phx-compiler` `#[cfg(test)]` only | Unit tests inside `phx-compiler` need a path |
//! | `tests/support` | Integration test crate | Pass tests under `phx-compiler/tests/` |
//! | `phx_test::sandbox` | `tests/phx-test` | Cross-crate integration, shared project sandboxes |
//!
//! Both this module and `tests/support` mirror the same logical layout under
//! `tests/cli/fixtures/...`; they differ only in temp-dir prefix and crate visibility.
//!
//! ## Layout on disk
//!
//! Each call writes under a **process-unique** subdirectory of [`std::env::temp_dir`]:
//!
//! ```text
//! {temp_dir}/phx_compiler_embed_{label}_{pid}_{n}/
//!   tests/cli/fixtures/{name}           ← single-file programs
//!   tests/cli/fixtures/{project}/       ← multi-file projects (phoenix.toml + sources)
//! ```
//!
//! The `{label}` is the embedded program or project name; `{n}` is a per-process counter so
//! parallel tests do not collide. Logical paths match on-disk CLI fixtures so module discovery
//! and `#import` resolution behave like CI.
//!
//! ## Lifecycle
//!
//! Temp directories are **not** deleted — returned paths remain valid until process exit.
//! Do not assume a stable path across calls; always use the [`PathBuf`] returned by the
//! materializer.

use std::io;
use std::path::{Path, PathBuf};

use phx_programs::{SingleFile, single::single_by_name};

/// Writes a single-file embedded program and returns its absolute path.
///
/// Looks up `name` in [`phx_programs::single`] (fixture basenames such as `"generic_fn.phx"` or
/// `"ref_local.phx"`), writes the source under a fresh temp root, and returns the full path to
/// the `.phx` file.
///
/// Typical use: pass the result to [`crate::check_file`] or [`crate::compile_to_module`] inside
/// a `#[cfg(test)]` module.
///
/// # Panics
///
/// Panics when `name` is not a known embedded program in [`phx_programs`], or when temp-dir
/// creation or file writes fail.
///
/// # Examples
///
/// ```no_run
/// let path = single_file_path("generic_fn.phx");
/// crate::check_file(&path).unwrap();
/// ```
pub fn single_file_path(name: &str) -> PathBuf {
    let program = single_by_name(name).unwrap_or_else(|| panic!("unknown program: {name}"));
    write_single(program)
}

/// Writes an embedded multi-file project and returns its project root directory.
///
/// Looks up `project_name` in [`phx_programs::projects`] (directory names such as `"std_smoke"`
/// or `"dynamic_array_grow"`), then materializes:
///
/// - `tests/cli/fixtures/{project_name}/phoenix.toml`
/// - every source file listed in the project spec (path deps included when present in the embed)
///
/// Entries ending in `.gitkeep` are skipped. The returned path is the **project root** (the
/// directory containing `phoenix.toml`), suitable for [`crate::project::discover_project`] and
/// project-aware [`crate::check_file`] when the entry file lives under `src/`.
///
/// # Panics
///
/// Panics when `project_name` is not a known embedded project in [`phx_programs`], or when
/// temp-dir creation or file writes fail.
///
/// # Examples
///
/// ```no_run
/// let root = project_root_path("std_smoke");
/// assert!(root.join("phoenix.toml").is_file());
/// ```
pub fn project_root_path(project_name: &str) -> PathBuf {
    let spec = phx_programs::projects::project_by_name(project_name)
        .unwrap_or_else(|| panic!("unknown project: {project_name}"));
    let root = io_unwrap(temp_root(spec.name), "mkdir temp root");
    let base = format!("tests/cli/fixtures/{}", spec.name);
    io_unwrap(
        write_at(&root, &format!("{base}/phoenix.toml"), spec.toml),
        "write embedded phoenix.toml",
    );
    for (rel, source) in spec.files {
        if !rel.ends_with(".gitkeep") {
            io_unwrap(
                write_at(&root, &format!("{base}/{rel}"), source),
                "write embedded source",
            );
        }
    }
    root.join(base)
}

/// Writes an embedded project and returns the path to its `src/main.phx` entry file.
///
/// Convenience wrapper around [`project_root_path`] for tests that compile or check the default
/// main entry without constructing `src/main.phx` manually.
///
/// # Panics
///
/// Same as [`project_root_path`].
///
/// # Examples
///
/// ```no_run
/// let main = project_main_path("dynamic_array_grow");
/// crate::compile_to_module(&main).unwrap();
/// ```
pub fn project_main_path(project_name: &str) -> PathBuf {
    project_root_path(project_name).join("src/main.phx")
}

fn write_single(program: &SingleFile) -> PathBuf {
    let root = io_unwrap(temp_root(program.name), "mkdir temp root");
    let logical = format!("tests/cli/fixtures/{}", program.name);
    io_unwrap(
        write_at(&root, &logical, program.source),
        "write embedded source",
    );
    root.join(logical)
}

fn temp_root(label: &str) -> io::Result<PathBuf> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "phx_compiler_embed_{label}_{}_{n}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

fn write_at(root: &Path, relative: &str, contents: &str) -> io::Result<()> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, contents)?;
    Ok(())
}

fn io_unwrap<T>(result: io::Result<T>, context: &str) -> T {
    match result {
        Ok(value) => value,
        Err(err) => panic!("{context}: {err}"),
    }
}

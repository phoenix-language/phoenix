//! Materialize embedded Phoenix programs from [`phx_programs`] to temp paths.
//!
//! Used by in-crate unit tests that call [`crate::check_file`] or other drivers against
//! fixture sources without copying files into the repo tree. Each call writes under a unique
//! subdirectory of the system temp dir with logical paths matching `tests/cli/fixtures/...`.
//!
//! Temp directories are not deleted — paths remain valid until process exit.

#![allow(clippy::expect_used)]

use std::path::{Path, PathBuf};

use phx_programs::{SingleFile, single::single_by_name};

/// Writes a single-file embedded program and returns its absolute path.
///
/// The returned path mirrors `tests/cli/fixtures/{name}` under a process-unique temp root.
/// Use with [`crate::check_file`] or [`crate::compile_to_module`].
///
/// # Panics
///
/// Panics when `name` is not a known embedded program in [`phx_programs`].
pub fn single_file_path(name: &str) -> PathBuf {
    let program = single_by_name(name).unwrap_or_else(|| panic!("unknown program: {name}"));
    write_single(program)
}

/// Writes an embedded multi-file project and returns its project root directory.
///
/// Materializes `phoenix.toml`, source files, and (when present) path-dependency layout under
/// `tests/cli/fixtures/{project_name}/` inside a process-unique temp root. `.gitkeep` entries
/// are skipped.
///
/// # Panics
///
/// Panics when `project_name` is not a known embedded project in [`phx_programs`], or when
/// temp-dir creation or file writes fail.
pub fn project_root_path(project_name: &str) -> PathBuf {
    let spec = phx_programs::projects::project_by_name(project_name)
        .unwrap_or_else(|| panic!("unknown project: {project_name}"));
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

/// Writes an embedded project and returns the path to its `src/main.phx` entry file.
///
/// Convenience wrapper around [`project_root_path`] for tests that compile or check the default
/// main entry.
///
/// # Panics
///
/// Same as [`project_root_path`].
pub fn project_main_path(project_name: &str) -> PathBuf {
    project_root_path(project_name).join("src/main.phx")
}

fn write_single(program: &SingleFile) -> PathBuf {
    let root = temp_root(program.name);
    let logical = format!("tests/cli/fixtures/{}", program.name);
    write_at(&root, &logical, program.source);
    root.join(logical)
}

fn temp_root(label: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "phx_compiler_embed_{label}_{}_{n}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("mkdir temp root");
    root
}

fn write_at(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&path, contents).expect("write embedded source");
}

//! Write embedded programs to temp paths for `check_file` in unit tests.
#![allow(clippy::expect_used)]

use std::path::{Path, PathBuf};

use phx_programs::{SingleFile, single::single_by_name};

/// Materialize a single-file program and return its path (leaked until process exit).
pub fn single_file_path(name: &str) -> PathBuf {
    let program = single_by_name(name).unwrap_or_else(|| panic!("unknown program: {name}"));
    write_single(program)
}
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

/// Materialize a project main entry path.
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

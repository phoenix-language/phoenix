//! Shared on-disk layout mirroring legacy `tests/cli/fixtures/` for path dependencies.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use phx_programs::ProjectSpec;

use crate::programs::lookup_project;

static SANDBOX: OnceLock<PathBuf> = OnceLock::new();
static MATERIALIZED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Root directory for lazily materialized embedded project fixtures.
pub fn fixture_sandbox_root() -> &'static Path {
    SANDBOX
        .get_or_init(|| {
            let root =
                std::env::temp_dir().join(format!("phx_fixture_sandbox_{}", std::process::id()));
            std::fs::create_dir_all(&root).expect("create fixture sandbox");
            root
        })
        .as_path()
}

/// Path to a project root under the shared sandbox (`tests/cli/fixtures/<name>/`).
pub fn sandbox_project_root(name: &str) -> PathBuf {
    ensure_materialized(name);
    let root = fixture_sandbox_root().join("tests/cli/fixtures").join(name);
    assert!(
        root.join("phoenix.toml").is_file(),
        "missing sandbox project: {}",
        root.display()
    );
    root
}

fn ensure_materialized(name: &str) {
    let materialized = MATERIALIZED.get_or_init(|| Mutex::new(HashSet::new()));
    let mut set = materialized
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if set.contains(name) {
        return;
    }
    materialize_project_tree(name, fixture_sandbox_root(), &mut set);
}

fn materialize_project_tree(name: &str, sandbox: &Path, materialized: &mut HashSet<String>) {
    if materialized.contains(name) {
        return;
    }
    let spec = lookup_project(name);
    for dep in path_deps_from_toml(spec.toml) {
        materialize_project_tree(&dep, sandbox, materialized);
    }
    write_project(spec, sandbox);
    materialized.insert(name.to_string());
}

fn path_deps_from_toml(toml: &str) -> Vec<String> {
    const PREFIX: &str = "path = \"../";
    let mut deps = Vec::new();
    let mut rest = toml;
    while let Some(idx) = rest.find(PREFIX) {
        let after = &rest[idx + PREFIX.len()..];
        if let Some(end) = after.find('"') {
            deps.push(after[..end].to_string());
        }
        rest = &rest[idx + 1..];
    }
    deps
}

pub(crate) fn write_project(spec: &ProjectSpec, sandbox: &Path) {
    let base = sandbox.join("tests/cli/fixtures").join(spec.name);
    write_file(&base.join("phoenix.toml"), spec.toml);
    for (rel, source) in spec.files {
        if rel.ends_with(".gitkeep") {
            if let Some(parent) = base.join(rel).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
        } else {
            write_file(&base.join(rel), source);
        }
    }
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, contents).expect("write");
}

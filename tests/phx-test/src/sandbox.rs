//! Shared on-disk layout mirroring legacy `tests/cli/fixtures/` for path dependencies.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use phx_programs::{PROJECTS, ProjectSpec};

use crate::programs::lookup_project;

static SANDBOX: OnceLock<PathBuf> = OnceLock::new();

/// Root directory containing all embedded project fixtures as siblings.
pub fn fixture_sandbox_root() -> &'static Path {
    SANDBOX
        .get_or_init(|| {
            let root =
                std::env::temp_dir().join(format!("phx_fixture_sandbox_{}", std::process::id()));
            std::fs::create_dir_all(&root).expect("create fixture sandbox");
            for spec in PROJECTS {
                write_project(spec, &root);
            }
            root
        })
        .as_path()
}

/// Path to a project root under the shared sandbox (`tests/cli/fixtures/<name>/`).
pub fn sandbox_project_root(name: &str) -> PathBuf {
    let spec = lookup_project(name);
    write_project(spec, fixture_sandbox_root());
    let root = fixture_sandbox_root()
        .join("tests/cli/fixtures")
        .join(spec.name);
    assert!(
        root.join("phoenix.toml").is_file(),
        "missing sandbox project: {}",
        root.display()
    );
    root
}

fn write_project(spec: &ProjectSpec, sandbox: &Path) {
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

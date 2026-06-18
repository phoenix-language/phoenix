//! Temporary on-disk workspaces for embedded test programs.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use phx_programs::{ModuleTree, ProjectSpec, SingleFile};

static WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Unique temp directory removed on drop.
#[derive(Debug)]
pub struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    /// Create a fresh directory under the system temp folder.
    pub fn new(label: &str) -> Self {
        let n = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let root = std::env::temp_dir().join(format!("phx_test_{label}_{pid}_{n}"));
        fs::create_dir_all(&root).unwrap_or_else(|e| panic!("create {}: {e}", root.display()));
        Self { root }
    }

    /// Root path of this workspace.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Write `relative` under the workspace root.
    pub fn write_file(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("mkdir {}: {e}", parent.display()));
        }
        fs::write(&path, contents).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }

    /// Materialize a single-file program under `tests/cli/fixtures/<name>`.
    pub fn write_single(&self, program: &SingleFile) -> PathBuf {
        let logical = format!("tests/cli/fixtures/{}", program.name);
        self.write_file(&logical, program.source);
        self.root().join(logical)
    }

    /// Materialize a module tree under `tests/cli/fixtures/modules/`.
    pub fn write_module_tree(&self, tree: &ModuleTree) -> PathBuf {
        for (rel, source) in tree.files {
            let logical = format!("tests/cli/fixtures/modules/{rel}");
            self.write_file(&logical, source);
        }
        self.root
            .join(format!("tests/cli/fixtures/modules/{}", tree.entry))
    }

    /// Materialize a project under `tests/cli/fixtures/<name>/`.
    pub fn write_project(&self, spec: &ProjectSpec) -> PathBuf {
        let base = format!("tests/cli/fixtures/{}", spec.name);
        self.write_file(&format!("{base}/phoenix.toml"), spec.toml);
        for (rel, source) in spec.files {
            if rel.ends_with(".gitkeep") {
                if let Some(parent) = self.root.join(&base).join(rel).parent() {
                    let _ = fs::create_dir_all(parent);
                }
            } else {
                self.write_file(&format!("{base}/{rel}"), source);
            }
        }
        self.root.join(base)
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Run `f` on a worker thread with a timeout (default 30s).
///
/// # Panics
///
/// Panics when `f` does not complete within `timeout` or when `f` panics.
pub fn run_with_timeout<T: Send + 'static>(
    label: &str,
    timeout: Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(timeout)
        .unwrap_or_else(|_| panic!("{label}: timed out after {timeout:?} (possible infinite loop)"))
}

/// Default VM run timeout for smoke tests.
pub const DEFAULT_RUN_TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_workspace_creates_and_cleans_up() {
        let path;
        {
            let ws = TempWorkspace::new("unit");
            path = ws.root().to_path_buf();
            assert!(path.is_dir());
        }
        assert!(!path.exists());
    }
}

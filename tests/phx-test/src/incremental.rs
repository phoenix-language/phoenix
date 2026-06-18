//! Helpers for incremental build tests that temporarily patch fixture sources.

use std::fs;
use std::path::{Path, PathBuf};

use phx_compiler::digest_bytes;

use crate::sandbox_lock::{project_fs_lock, sandbox_project_name_from_fixture_path};

/// SHA-256 digest of file contents (hex), matching build artifact hashing.
pub fn file_digest(path: &Path) -> String {
    let bytes = fs::read(path).expect("read");
    digest_bytes(&bytes)
}

/// Temporarily replaces a fixture file; restores original contents on drop.
#[derive(Debug)]
pub struct FixturePatch {
    path: PathBuf,
    original: String,
    project: String,
}

impl FixturePatch {
    /// Read `path`, apply `f` to produce new contents, write, and schedule restore on drop.
    pub fn replace(path: &Path, f: impl FnOnce(&str) -> String) -> Self {
        let project = sandbox_project_name_from_fixture_path(path).unwrap_or_else(|| {
            panic!(
                "fixture patch path is not under tests/cli/fixtures/<project>/: {}",
                path.display()
            )
        });
        let _lock = project_fs_lock(&project);
        let original =
            fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let patched = f(&original);
        fs::write(path, &patched).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        Self {
            path: path.to_path_buf(),
            original,
            project,
        }
    }
}

impl Drop for FixturePatch {
    fn drop(&mut self) {
        let _lock = project_fs_lock(&self.project);
        fs::write(&self.path, &self.original)
            .unwrap_or_else(|e| panic!("restore {}: {e}", self.path.display()));
    }
}

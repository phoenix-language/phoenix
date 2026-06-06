//! Helpers for incremental build tests that temporarily patch fixture sources.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use phx_compiler::digest_bytes;

static FIXTURE_FS_LOCK: Mutex<()> = Mutex::new(());

/// Serializes tests that mutate or rebuild shared on-disk fixtures.
pub fn fixture_fs_lock() -> MutexGuard<'static, ()> {
    FIXTURE_FS_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

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
}

impl FixturePatch {
    /// Read `path`, apply `f` to produce new contents, write, and schedule restore on drop.
    pub fn replace(path: &Path, f: impl FnOnce(&str) -> String) -> Self {
        let _lock = fixture_fs_lock();
        let original =
            fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let patched = f(&original);
        fs::write(path, &patched).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        Self {
            path: path.to_path_buf(),
            original,
        }
    }
}

impl Drop for FixturePatch {
    fn drop(&mut self) {
        let _lock = fixture_fs_lock();
        fs::write(&self.path, &self.original)
            .unwrap_or_else(|e| panic!("restore {}: {e}", self.path.display()));
    }
}

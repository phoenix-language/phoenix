//! Content digests for incremental builds (stdlib only).

use std::hash::{Hash, Hasher};

/// Returns a lowercase hex digest of `bytes` (stable for incremental invalidation).
#[must_use]
pub fn digest_bytes(bytes: &[u8]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Digest of a file's contents.
///
/// # Errors
///
/// Returns I/O errors from `read_to_end`.
pub fn digest_file(path: &std::path::Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(digest_bytes(&bytes))
}

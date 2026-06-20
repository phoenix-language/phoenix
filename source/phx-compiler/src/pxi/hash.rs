//! Content digests for `.pxi` incremental builds (stdlib only).
//!
//! ## Pass role
//!
//! Computes stable fingerprints for source files and serialized `.pxi` payloads. The build
//! driver stores these in [`PxiFile::source_hash`](super::format::PxiFile::source_hash) and
//! [`PxiDependency::pxi_hash`](super::format::PxiDependency::pxi_hash) so importers can skip
//! re-type-checking when dependency interfaces are unchanged
//! ([`PxiFile::source_is_fresh`](super::format::PxiFile::source_is_fresh)).
//!
//! ## Algorithm
//!
//! Uses [`std::collections::hash_map::DefaultHasher`] over the raw byte slice. The digest is a
//! fixed-width lowercase hex string suitable for manifest comparison, not a cryptographic hash.

use std::hash::{Hash, Hasher};

/// Returns a lowercase hex digest of `bytes`.
///
/// The same input always yields the same digest on a given host (used for incremental
/// invalidation, not security).
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

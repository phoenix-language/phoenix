//! `build/manifest.json` — incremental rebuild metadata (M2).
//!
//! Persists per-module source digests, `.pxi` interface hashes, and artifact paths under
//! the workspace `build/` directory. The build driver reads the previous manifest at the
//! start of a build and writes an updated copy after artifact emission so unchanged modules
//! can be skipped on the next run.
//!
//! ## Pipeline position
//!
//! Written by [`super::driver::artifacts::write_interfaces_and_manifest`] and
//! [`super::driver::artifacts::write_interfaces_and_collect_objects`]; read at the start of
//! [`super::driver::package::build_project`]. Freshness checks in
//! [`super::driver::incremental`] compare live digests against stored records via
//! [`module_is_up_to_date`].
//!
//! ## On-disk format
//!
//! JSON object with top-level `entry`, `bin_path`, and a `modules` map keyed by logical
//! module path. Each module entry holds `source`, `source_hash`, `pxi_hash`, `phx0_path`,
//! and `pxi_path` strings (paths stored relative to `build_root` when possible — see
//! [`store_path_relative_to`]).
//!
//! Parsing is intentionally lenient: [`BuildManifest::read`] uses a lightweight extractor
//! rather than a strict JSON deserializer, so partially malformed files may yield missing
//! fields rather than a hard error.
//!
//! ## Freshness model
//!
//! A module is **up to date** when:
//!
//! 1. A [`ManifestModule`] exists for its logical path.
//! 2. Live source digest matches `source_hash`.
//! 3. Every imported dependency's live `.pxi` digest matches the recorded `pxi_hash`.
//! 4. Both `.phx0` and `.pxi` artifact paths resolve to existing files under `build_root`.
//!
//! ## Public API
//!
//! | Item | Role |
//! | --- | --- |
//! | [`ManifestModule`] | Per-module record stored in the manifest |
//! | [`BuildManifest`] | Parsed manifest; [`BuildManifest::read`] / [`BuildManifest::write`] |
//! | [`module_is_up_to_date`] | Incremental skip predicate for one workspace module |
//! | [`store_path_relative_to`] | Normalize artifact paths before writing |
//! | [`resolve_manifest_path`] | Resolve stored paths when reading artifacts |
//! | [`record_pxi_hash`] | Digest a `.pxi` file for dependency edges |

use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use crate::pxi::digest_bytes;

/// Per-module record in `build/manifest.json`.
///
/// One entry per compiled workspace module. Paths (`source`, `phx0_path`, `pxi_path`) are
/// stored as project- or build-root-relative strings when [`store_path_relative_to`] was
/// used at write time; hashes are hex digests from [`crate::pxi::digest_file`] /
/// [`record_pxi_hash`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestModule {
    /// Logical module path (map key in the manifest), e.g. `std::core::option`.
    pub logical_path: String,
    /// Project-relative path to the `.phx` source file.
    pub source: String,
    /// Hex digest of source file bytes at last successful compile.
    pub source_hash: String,
    /// Hex digest of the emitted `.pxi` interface file.
    pub pxi_hash: String,
    /// Stored path to the per-module `.phx0` object file.
    pub phx0_path: String,
    /// Stored path to the per-module `.pxi` interface file.
    pub pxi_path: String,
}

/// Parsed `build/manifest.json` for a workspace package.
///
/// Holds the crate entry logical path, linked binary location, and all per-module records
/// from the last successful build. An empty [`BuildManifest::default`] is used on first
/// build when no manifest file exists yet.
#[derive(Debug, Clone, Default)]
pub struct BuildManifest {
    /// Logical module path of the package entry point (matches `phoenix.toml` `[[bin]]`).
    pub entry: String,
    /// Stored path to the linked output binary (`.phx0` or final artifact).
    pub bin_path: String,
    /// Per-module records keyed by logical path (`ManifestModule::logical_path`).
    pub modules: HashMap<String, ManifestModule>,
}

impl BuildManifest {
    /// Loads a manifest from `path` when the file exists and is readable.
    ///
    /// Returns `None` when `path` is missing or cannot be read (including permission
    /// errors). Malformed JSON is parsed leniently: missing keys become empty strings and
    /// unknown module fields are ignored rather than failing the load.
    ///
    /// # Panics
    ///
    /// Never panics on malformed manifest content.
    pub fn read(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        Some(parse_manifest(&text))
    }

    /// Serializes this manifest to JSON and writes it to `path`.
    ///
    /// Creates parent directories when needed. Module keys are emitted in arbitrary
    /// `HashMap` iteration order; round-tripping through [`BuildManifest::read`] preserves
    /// all fields but not key ordering.
    ///
    /// # Errors
    ///
    /// Returns [`std::io::Error`] when parent directory creation or the file write fails.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_json())
    }

    fn to_json(&self) -> String {
        let mut out = String::from("{\n");
        let _ = writeln!(out, "  \"entry\": {},", json_str(&self.entry));
        let _ = writeln!(out, "  \"bin_path\": {},", json_str(&self.bin_path));
        out.push_str("  \"modules\": {\n");
        let keys: Vec<_> = self.modules.keys().collect();
        for (i, key) in keys.iter().enumerate() {
            let m = &self.modules[*key];
            let comma = if i + 1 < keys.len() { "," } else { "" };
            let _ = writeln!(out, "    {}: {{", json_str(key));
            let _ = writeln!(out, "      \"source\": {},", json_str(&m.source));
            let _ = writeln!(out, "      \"source_hash\": {},", json_str(&m.source_hash));
            let _ = writeln!(out, "      \"pxi_hash\": {},", json_str(&m.pxi_hash));
            let _ = writeln!(out, "      \"phx0_path\": {},", json_str(&m.phx0_path));
            let _ = writeln!(out, "      \"pxi_path\": {}", json_str(&m.pxi_path));
            let _ = writeln!(out, "    }}{comma}");
        }
        out.push_str("  }\n}\n");
        out
    }
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        if c == '"' || c == '\\' {
            o.push('\\');
        }
        o.push(c);
    }
    o.push('"');
    o
}

fn parse_manifest(text: &str) -> BuildManifest {
    let mut manifest = BuildManifest::default();
    if let Some(entry) = extract_string(text, "entry") {
        manifest.entry = entry;
    }
    if let Some(bin) = extract_string(text, "bin_path") {
        manifest.bin_path = bin;
    }
    let modules_text = modules_section(text).unwrap_or(text);
    for (logical, ()) in extract_module_keys(text) {
        let needle = format!("\"{logical}\": {{");
        let Some(pos) = modules_text.find(&needle) else {
            continue;
        };
        let chunk = &modules_text[pos..];
        manifest.modules.insert(
            logical.clone(),
            ManifestModule {
                logical_path: logical.clone(),
                source: extract_string(chunk, "source").unwrap_or_default(),
                source_hash: extract_string(chunk, "source_hash").unwrap_or_default(),
                pxi_hash: extract_string(chunk, "pxi_hash").unwrap_or_default(),
                phx0_path: extract_string(chunk, "phx0_path").unwrap_or_default(),
                pxi_path: extract_string(chunk, "pxi_path").unwrap_or_default(),
            },
        );
    }
    manifest
}

fn extract_string(text: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":");
    let pos = text.find(&pat)? + pat.len();
    let rest = text[pos..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let mut out = String::new();
    for c in rest[1..].chars() {
        if c == '"' {
            return Some(out);
        }
        if c == '\\' {
            continue;
        }
        out.push(c);
    }
    None
}

fn modules_section(text: &str) -> Option<&str> {
    let mods = text.find("\"modules\"")?;
    let after = &text[mods..];
    let open = after.find('{')? + 1;
    let mut depth = 1i32;
    let bytes = after.as_bytes();
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&after[open..i]);
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_module_keys(text: &str) -> Vec<(String, ())> {
    let mut out = Vec::new();
    let Some(mods) = text.find("\"modules\"") else {
        return out;
    };
    let slice = &text[mods..];
    for line in slice.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('"')
            && trimmed.ends_with(": {")
            && let Some(name) = trimmed
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix("\": {"))
            && !name.is_empty()
            && name != "modules"
        {
            out.push((name.to_owned(), ()));
        }
    }
    out
}

/// Stores `path` relative to `build_root` when it is under that prefix.
///
/// Used when writing manifest artifact paths so manifests remain valid when the project
/// directory moves. If `path` is not prefixed by `build_root`, returns
/// [`Path::display`] of `path` unchanged (typically an absolute path).
#[must_use]
pub fn store_path_relative_to(build_root: &Path, path: &Path) -> String {
    path.strip_prefix(build_root)
        .map_or_else(|_| path.display().to_string(), |p| p.display().to_string())
}

/// Resolves a path string stored in the manifest to a filesystem location.
///
/// Resolution order:
///
/// 1. If `stored` is absolute, return it as-is.
/// 2. Otherwise join `build_root` with `stored`; if that file exists, return it.
/// 3. Otherwise if `stored` exists relative to the process current directory, return that.
/// 4. Otherwise return `build_root.join(stored)` even when the file is missing (callers
///    such as [`module_is_up_to_date`] use [`Path::is_file`] to detect staleness).
#[must_use]
pub fn resolve_manifest_path(build_root: &Path, stored: &str) -> PathBuf {
    let p = Path::new(stored);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    let from_build = build_root.join(p);
    if from_build.is_file() {
        return from_build;
    }
    if p.is_file() {
        return p.to_path_buf();
    }
    from_build
}

/// Returns `true` when module `logical` can be skipped for incremental rebuild.
///
/// Compares `source_hash` and each `(dep_logical, dep_pxi_hash)` pair against the last
/// successful build recorded in `manifest`. Also verifies artifact files still exist on disk.
///
/// Returns `false` on the first failed check. Used by
/// [`super::driver::incremental::workspace_stale_modules`] and per-module skip logic in
/// [`super::driver::artifacts`].
///
/// # Panics
///
/// Never panics.
pub fn module_is_up_to_date(
    manifest: &BuildManifest,
    build_root: &Path,
    logical: &str,
    source_hash: &str,
    dep_pxi_hashes: &[(String, String)],
) -> bool {
    let Some(rec) = manifest.modules.get(logical) else {
        return false;
    };
    if rec.source_hash != source_hash {
        return false;
    }
    for (dep_path, hash) in dep_pxi_hashes {
        let Some(dep_rec) = manifest.modules.get(dep_path) else {
            return false;
        };
        if &dep_rec.pxi_hash != hash {
            return false;
        }
    }
    resolve_manifest_path(build_root, &rec.phx0_path).is_file()
        && resolve_manifest_path(build_root, &rec.pxi_path).is_file()
}

/// Returns the hex digest of a `.pxi` file for manifest dependency edges.
///
/// Reads the full file and passes bytes to [`crate::pxi::digest_bytes`]. When the file
/// cannot be read, returns an empty string (callers treat that as stale / not up to date).
///
/// # Panics
///
/// Never panics on missing or unreadable `pxi_path`.
#[must_use]
pub fn record_pxi_hash(pxi_path: &Path) -> String {
    std::fs::read(pxi_path)
        .map(|b| digest_bytes(&b))
        .unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest_std_module_not_confused_with_entry() {
        let text = r#"{
  "entry": "std",
  "bin_path": "build/deps/std/lib/std.phx0",
  "modules": {
    "std::core::option": {
      "source": "src/core/option.phx",
      "source_hash": "a",
      "pxi_hash": "b",
      "phx0_path": "phx0/std/core/option.phx0",
      "pxi_path": "pxi/std/core/option.pxi"
    },
    "std": {
      "source": "src/lib.phx",
      "source_hash": "c",
      "pxi_hash": "d",
      "phx0_path": "phx0/std.phx0",
      "pxi_path": "pxi/std.pxi"
    }
  }
}
"#;
        let manifest = parse_manifest(text);
        let std_mod = manifest.modules.get("std").expect("std module");
        assert_eq!(std_mod.pxi_path, "pxi/std.pxi");
        assert_eq!(std_mod.source, "src/lib.phx");
    }

    #[test]
    fn parse_manifest_single_segment_module_key() {
        let text = r#"{
  "entry": "math",
  "bin_path": "lib/math.phx0",
  "modules": {
    "math": {
      "source": "src/lib.phx",
      "source_hash": "abc",
      "pxi_hash": "def",
      "phx0_path": "phx0/math.phx0",
      "pxi_path": "pxi/math.pxi"
    }
  }
}
"#;
        let manifest = parse_manifest(text);
        assert_eq!(manifest.modules.len(), 1);
        assert!(manifest.modules.contains_key("math"));
    }
}

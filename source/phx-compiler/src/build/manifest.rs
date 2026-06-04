//! `build/manifest.json` for incremental rebuilds.

use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;

use crate::pxi::digest_bytes;

/// Per-module record in the build manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestModule {
    /// Logical module path.
    pub logical_path: String,
    /// Project-relative source path.
    pub source: String,
    /// Digest of source bytes.
    pub source_hash: String,
    /// Digest of `.pxi` file.
    pub pxi_hash: String,
    /// Path to `.phx0` object file.
    pub phx0_path: String,
    /// Path to `.pxi` interface file.
    pub pxi_path: String,
}

/// Parsed build manifest.
#[derive(Debug, Clone, Default)]
pub struct BuildManifest {
    /// Entry logical module path.
    pub entry: String,
    /// Linked binary path.
    pub bin_path: String,
    /// Per-module records keyed by logical path.
    pub modules: HashMap<String, ManifestModule>,
}

impl BuildManifest {
    /// Reads `path` if it exists.
    pub fn read(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        Some(parse_manifest(&text))
    }

    /// Writes manifest JSON to `path`.
    ///
    /// # Errors
    ///
    /// I/O errors.
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
    for (logical, ()) in extract_module_keys(text) {
        let prefix = format!("\"{logical}\"");
        if let Some(pos) = text.find(&prefix) {
            let chunk = &text[pos..];
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

fn extract_module_keys(text: &str) -> Vec<(String, ())> {
    let mut out = Vec::new();
    let Some(mods) = text.find("\"modules\"") else {
        return out;
    };
    let slice = &text[mods..];
    for line in slice.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('"')
            && trimmed.contains("::")
            && trimmed.ends_with(": {")
            && let Some(name) = trimmed
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix("\": {"))
        {
            out.push((name.to_owned(), ()));
        }
    }
    out
}

/// Returns true when module `logical` does not need recompilation.
pub fn module_is_up_to_date(
    manifest: &BuildManifest,
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
    std::path::Path::new(&rec.phx0_path).is_file() && std::path::Path::new(&rec.pxi_path).is_file()
}

/// Hash of manifest module record for dependency edges.
#[must_use]
pub fn record_pxi_hash(pxi_path: &Path) -> String {
    std::fs::read(pxi_path)
        .map(|b| digest_bytes(&b))
        .unwrap_or_default()
}

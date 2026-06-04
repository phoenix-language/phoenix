//! `.pxi` JSON read/write (v1 signatures, v2 structured types).

use std::fmt::Write;
use std::path::Path;

use super::hash::digest_bytes;
use super::type_ast::{PxiType, parse_type_value};
use crate::resolver::DefKind;

/// One exported symbol in a `.pxi` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PxiExport {
    /// Stable id (`logical_module::name::kind`) for separate compilation.
    pub export_id: String,
    /// Interned symbol name (stored as string in file).
    pub name: String,
    /// `fn`, `struct`, `enum`, etc.
    pub kind: String,
    /// Stable type signature string.
    pub signature: String,
    /// Structured type (format v2).
    pub ty: Option<PxiType>,
}

/// Builds a stable export id for `.pxi` and link maps.
#[must_use]
pub fn stable_export_id(logical_module: &str, name: &str, kind: &str) -> String {
    format!("{logical_module}::{name}::{kind}")
}

/// One dependency entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PxiDependency {
    /// Logical module path.
    pub logical_module: String,
    /// Digest of the dependency `.pxi` at compile time.
    pub pxi_hash: String,
}

/// Parsed `.pxi` interface (format version 1 or 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PxiFile {
    /// `1` (signatures only) or `2` (structured `type` on exports).
    pub format_version: u32,
    /// Logical module path (`a::b`).
    pub logical_module: String,
    /// Digest of corresponding `.phx` source.
    pub source_hash: String,
    /// Optional package origin (future).
    pub origin: Option<String>,
    /// `pub` exports.
    pub exports: Vec<PxiExport>,
    /// Direct module dependencies.
    pub dependencies: Vec<PxiDependency>,
}

impl PxiFile {
    /// Serializes to JSON bytes.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        let _ = writeln!(out, "  \"format_version\": {},", self.format_version);
        let _ = writeln!(
            out,
            "  \"logical_module\": {},",
            json_string(&self.logical_module)
        );
        let _ = writeln!(
            out,
            "  \"source_hash\": {},",
            json_string(&self.source_hash)
        );
        match &self.origin {
            Some(o) => {
                let _ = writeln!(out, "  \"origin\": {},", json_string(o));
            }
            None => out.push_str("  \"origin\": null,\n"),
        }
        out.push_str("  \"exports\": [\n");
        for (i, e) in self.exports.iter().enumerate() {
            let comma = if i + 1 < self.exports.len() { "," } else { "" };
            if let Some(ty) = &e.ty {
                let _ = writeln!(
                    out,
                    "    {{\"export_id\": {}, \"name\": {}, \"kind\": {}, \"signature\": {}, \"type\": {}}}{comma}",
                    json_string(&e.export_id),
                    json_string(&e.name),
                    json_string(&e.kind),
                    json_string(&e.signature),
                    ty.to_json()
                );
            } else {
                let _ = writeln!(
                    out,
                    "    {{\"export_id\": {}, \"name\": {}, \"kind\": {}, \"signature\": {}}}{comma}",
                    json_string(&e.export_id),
                    json_string(&e.name),
                    json_string(&e.kind),
                    json_string(&e.signature)
                );
            }
        }
        out.push_str("  ],\n");
        out.push_str("  \"dependencies\": [\n");
        for (i, d) in self.dependencies.iter().enumerate() {
            let comma = if i + 1 < self.dependencies.len() {
                ","
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "    {{\"logical_module\": {}, \"pxi_hash\": {}}}{comma}",
                json_string(&d.logical_module),
                json_string(&d.pxi_hash)
            );
        }
        out.push_str("  ]\n");
        out.push('}');
        out
    }

    /// Writes this interface to `path` (creates parent directories).
    ///
    /// # Errors
    ///
    /// I/O errors.
    pub fn write_to_path(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_json())
    }

    /// Parses v1 JSON from `text`.
    ///
    /// # Errors
    ///
    /// Returns [`PxiError`] on unsupported version or malformed JSON.
    pub fn parse(text: &str) -> Result<Self, PxiError> {
        parse_inner(text)
    }

    /// Reads and parses a `.pxi` file.
    ///
    /// # Errors
    ///
    /// I/O or parse failures.
    pub fn read_from_path(path: &Path) -> Result<Self, PxiError> {
        let text = std::fs::read_to_string(path).map_err(|e| PxiError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        Self::parse(&text)
    }

    /// Returns true when `source_path` bytes match `source_hash`.
    #[must_use]
    pub fn source_is_fresh(&self, source_path: &Path) -> bool {
        std::fs::read(source_path).is_ok_and(|b| digest_bytes(&b) == self.source_hash)
    }

    /// Digest of this file's canonical JSON (for dependency tracking).
    #[allow(dead_code)]
    #[must_use]
    pub fn self_hash(&self) -> String {
        digest_bytes(self.to_json().as_bytes())
    }
}

/// `.pxi` parse/load errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PxiError {
    /// Unsupported `format_version`.
    UnsupportedVersion {
        /// Version found.
        found: u32,
    },
    /// Malformed JSON.
    Parse {
        /// Detail.
        message: String,
    },
    /// I/O failure.
    Io {
        /// Path.
        path: String,
        /// Message.
        message: String,
    },
}

impl std::fmt::Display for PxiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion { found } => {
                write!(f, "unsupported .pxi format version {found}")
            }
            Self::Parse { message } => write!(f, "invalid .pxi: {message}"),
            Self::Io { path, message } => write!(f, "I/O reading {path}: {message}"),
        }
    }
}

impl std::error::Error for PxiError {}

/// Maps [`DefKind`] to `.pxi` export kind string.
#[must_use]
pub fn def_kind_to_pxi(kind: DefKind) -> &'static str {
    match kind {
        DefKind::Fn => "fn",
        DefKind::Struct => "struct",
        DefKind::Enum => "enum",
        DefKind::TypeAlias => "type",
        DefKind::Trait => "trait",
        DefKind::Const => "const",
        DefKind::Var => "var",
        DefKind::EnumVariant => "variant",
        DefKind::StructField
        | DefKind::Param
        | DefKind::Local
        | DefKind::Impl
        | DefKind::GenericParam
        | DefKind::Closure
        | DefKind::TraitAssocType => "other",
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn parse_inner(text: &str) -> Result<PxiFile, PxiError> {
    let version = extract_u32(text, "format_version").ok_or_else(|| PxiError::Parse {
        message: "missing format_version".to_owned(),
    })?;
    if version != 1 && version != 2 {
        return Err(PxiError::UnsupportedVersion { found: version });
    }
    if text.contains("\"module_path\"") {
        return Err(PxiError::Parse {
            message: "legacy .pxi field module_path is not supported; use logical_module"
                .to_owned(),
        });
    }
    let logical_module = extract_string(text, "logical_module").ok_or_else(|| PxiError::Parse {
        message: "missing logical_module".to_owned(),
    })?;
    let source_hash = extract_string(text, "source_hash").ok_or_else(|| PxiError::Parse {
        message: "missing source_hash".to_owned(),
    })?;
    let origin = extract_optional_string(text, "origin");
    let exports = parse_exports(&logical_module, text);
    let dependencies = parse_dependencies(text);
    Ok(PxiFile {
        format_version: version,
        logical_module,
        source_hash,
        origin,
        exports,
        dependencies,
    })
}

fn extract_u32(text: &str, key: &str) -> Option<u32> {
    let pat = format!("\"{key}\":");
    let pos = text.find(&pat)? + pat.len();
    let rest = text[pos..].trim_start();
    let num: String = rest.chars().take_while(char::is_ascii_digit).collect();
    num.parse().ok()
}

fn extract_string(text: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":");
    let pos = text.find(&pat)? + pat.len();
    let rest = text[pos..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    parse_json_string(&rest[1..])
}

fn extract_optional_string(text: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":");
    let pos = text.find(&pat)? + pat.len();
    let rest = text[pos..].trim_start();
    if rest.starts_with("null") {
        return None;
    }
    if !rest.starts_with('"') {
        return None;
    }
    parse_json_string(&rest[1..])
}

fn parse_json_string(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '"' {
            return Some(out);
        }
        if c == '\\' {
            let next = chars.next()?;
            out.push(match next {
                '"' => '"',
                '\\' => '\\',
                'n' => '\n',
                other => other,
            });
        } else {
            out.push(c);
        }
    }
    None
}

fn parse_exports(logical_module: &str, text: &str) -> Vec<PxiExport> {
    let Some(start) = text.find("\"exports\"") else {
        return Vec::new();
    };
    let Some(arr_start) = text[start..].find('[') else {
        return Vec::new();
    };
    let slice = &text[start + arr_start..];
    let mut exports = Vec::new();
    let mut search = slice;
    while let Some(name_pos) = search.find("\"name\"") {
        let chunk = &search[name_pos..];
        if let (Some(name), Some(kind), Some(sig)) = (
            extract_field_string(chunk, "name"),
            extract_field_string(chunk, "kind"),
            extract_field_string(chunk, "signature"),
        ) {
            let export_id = extract_field_string(chunk, "export_id")
                .unwrap_or_else(|| stable_export_id(logical_module, &name, &kind));
            let ty = parse_export_type(chunk);
            exports.push(PxiExport {
                export_id,
                name,
                kind,
                signature: sig,
                ty,
            });
        }
        search = &search[name_pos + 6..];
    }
    exports
}

fn parse_dependencies(text: &str) -> Vec<PxiDependency> {
    let Some(start) = text.find("\"dependencies\"") else {
        return Vec::new();
    };
    let Some(arr_start) = text[start..].find('[') else {
        return Vec::new();
    };
    let slice = &text[start + arr_start..];
    let mut deps = Vec::new();
    let mut search = slice;
    while let Some(pos) = search.find("\"logical_module\"") {
        let chunk = &search[pos..];
        if let (Some(logical_module), Some(pxi_hash)) = (
            extract_field_string(chunk, "logical_module"),
            extract_field_string(chunk, "pxi_hash"),
        ) {
            deps.push(PxiDependency {
                logical_module,
                pxi_hash,
            });
        }
        search = &search[pos + 16..];
    }
    deps
}

fn parse_export_type(chunk: &str) -> Option<PxiType> {
    let pat = "\"type\":";
    let pos = chunk.find(pat)? + pat.len();
    parse_type_value(&chunk[pos..])
}

fn extract_field_string(chunk: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":");
    let pos = chunk.find(&pat)? + pat.len();
    let rest = chunk[pos..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    parse_json_string(&rest[1..])
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let pxi = PxiFile {
            format_version: 1,
            logical_module: "util::math".to_owned(),
            source_hash: "abc".to_owned(),
            origin: None,
            exports: vec![PxiExport {
                export_id: "util::math::add::fn".to_owned(),
                name: "add".to_owned(),
                kind: "fn".to_owned(),
                signature: "(s32, s32) => s32".to_owned(),
                ty: None,
            }],
            dependencies: vec![PxiDependency {
                logical_module: "core".to_owned(),
                pxi_hash: "def".to_owned(),
            }],
        };
        let json = pxi.to_json();
        let back = PxiFile::parse(&json).unwrap();
        assert_eq!(back.logical_module, "util::math");
        assert_eq!(back.exports.len(), 1);
    }
}

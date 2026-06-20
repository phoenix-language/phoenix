//! Parse and serialize `.pxi` interface files (format v1 and v2).
//!
//! ## On-disk shape
//!
//! A `.pxi` file is JSON with top-level `format_version`, `logical_module`, `source_hash`,
//! optional `origin`, an `exports` array, and a `dependencies` array. Each export carries a
//! stable [`PxiExport::export_id`], human-readable [`PxiExport::signature`], and (v2) optional
//! structured [`PxiExport::ty`].
//!
//! ## Version support
//!
//! [`PxiFile::parse`] accepts `format_version` `1` or `2`. Legacy field `module_path` is
//! rejected. Writers in the current compiler emit v2 via [`crate::pxi::build_pxi_for_module`].
//!
//! See `docs/design/features/pxi-format.md` for the full schema and cross-crate generic rules.

use std::fmt::Write;
use std::path::Path;

use super::hash::digest_bytes;
use super::type_ast::{PxiType, parse_type_value};
use crate::resolver::DefKind;

/// Language-item marker on a `.pxi` export (format v2).
///
/// Present when the export is a compiler-known item such as `Option` or an intrinsic;
/// ordinary user exports omit this field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PxiLangItem {
    /// Stable language item name.
    pub name: String,
    /// Item category (`intrinsic`, `enum`, `trait`, `variant`).
    pub kind: String,
}

/// One exported symbol recorded in a `.pxi` file.
///
/// Corresponds to a `pub` item (or link-required impl method) from the module source.
/// v2 exports include structured [`Self::ty`]; v1 exports carry [`Self::signature`] only.
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
    /// Global PHX0 `function_id` for `fn` exports (format v2, optional for backward compat).
    pub function_id: Option<u32>,
    /// Compiler language item marker when present.
    pub lang_item: Option<PxiLangItem>,
}

/// Builds a stable export id: `logical_module::name::kind`.
///
/// Used in `.pxi` export records and link-time export maps. Generic specializations use
/// mangled names in `name` (for example `id$s32`); see the design doc for mangling rules.
#[must_use]
pub fn stable_export_id(logical_module: &str, name: &str, kind: &str) -> String {
    format!("{logical_module}::{name}::{kind}")
}

/// One direct module dependency listed in a `.pxi` file.
///
/// `pxi_hash` is the content digest of the dependency's `.pxi` at compile time; a changed
/// hash invalidates incremental builds of importers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PxiDependency {
    /// Logical module path.
    pub logical_module: String,
    /// Digest of the dependency `.pxi` at compile time.
    pub pxi_hash: String,
}

/// Parsed `.pxi` interface (format version 1 or 2).
///
/// Construct with [`PxiFile::parse`] or [`PxiFile::read_from_path`]; emit via
/// [`crate::pxi::build_pxi_for_module`] during project builds.
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
    /// Serializes this interface to canonical JSON text.
    ///
    /// The output is suitable for writing to disk and for computing [`Self::self_hash`].
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
            let fn_id = e
                .function_id
                .map(|id| format!(", \"function_id\": {id}"))
                .unwrap_or_default();
            if let Some(ty) = &e.ty {
                let lang_item = e
                    .lang_item
                    .as_ref()
                    .map(|li| {
                        format!(
                            ", \"lang_item\": {{\"name\": {}, \"kind\": {}}}",
                            json_string(&li.name),
                            json_string(&li.kind)
                        )
                    })
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "    {{\"export_id\": {}, \"name\": {}, \"kind\": {}, \"signature\": {}{fn_id}{lang_item}, \"type\": {}}}{comma}",
                    json_string(&e.export_id),
                    json_string(&e.name),
                    json_string(&e.kind),
                    json_string(&e.signature),
                    ty.to_json()
                );
            } else {
                let lang_item = e
                    .lang_item
                    .as_ref()
                    .map(|li| {
                        format!(
                            ", \"lang_item\": {{\"name\": {}, \"kind\": {}}}",
                            json_string(&li.name),
                            json_string(&li.kind)
                        )
                    })
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "    {{\"export_id\": {}, \"name\": {}, \"kind\": {}, \"signature\": {}{fn_id}{lang_item}}}{comma}",
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
    /// Returns `PxiError` on unsupported version, malformed JSON, or invalid
    /// `exports` / `dependencies` arrays.
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

    /// Returns `true` when the bytes at `source_path` match [`Self::source_hash`].
    ///
    /// Used by the build driver to skip re-emitting interfaces for unchanged modules and to
    /// decide whether a dependency `.pxi` still describes its source.
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

/// `.pxi` parse and load failures.
///
/// Malformed JSON, unsupported versions, and structural array errors surface here rather
/// than as partial parse results.
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

/// Maps a resolver [`DefKind`] to the `.pxi` export `kind` string.
///
/// Function-like defs map to `"fn"` or `"extern_fn"`; aggregate defs map to `"struct"`,
/// `"enum"`, or `"type"`. Internal-only kinds (locals, params, impl blocks) map to `"other"`.
#[must_use]
pub fn def_kind_to_pxi(kind: DefKind) -> &'static str {
    match kind {
        DefKind::Fn | DefKind::ImplMethod => "fn",
        DefKind::ExternFn => "extern_fn",
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
    let exports = parse_exports(&logical_module, text)?;
    let dependencies = parse_dependencies(text)?;
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

fn malformed_array(field_name: &str, detail: &str) -> PxiError {
    PxiError::Parse {
        message: format!("malformed {field_name}: {detail}"),
    }
}

fn parse_json_object_array<T, F>(
    text: &str,
    field_name: &str,
    mut parse_elem: F,
) -> Result<Vec<T>, PxiError>
where
    F: FnMut(&str) -> Result<T, PxiError>,
{
    let key = format!("\"{field_name}\"");
    let start = text.find(&key).ok_or_else(|| PxiError::Parse {
        message: format!("missing {field_name}"),
    })?;
    let arr_start = text[start..]
        .find('[')
        .ok_or_else(|| malformed_array(field_name, "expected '['"))?;
    let slice = &text[start + arr_start + 1..];
    let mut items = Vec::new();
    let mut i = 0usize;
    let mut closed = false;
    while i < slice.len() {
        while i < slice.len()
            && (slice.as_bytes()[i].is_ascii_whitespace() || slice.as_bytes()[i] == b',')
        {
            i += 1;
        }
        if i >= slice.len() {
            return Err(malformed_array(
                field_name,
                &format!("truncated {field_name} array"),
            ));
        }
        if slice.as_bytes()[i] == b']' {
            closed = true;
            break;
        }
        if slice.as_bytes()[i] != b'{' {
            return Err(malformed_array(field_name, "expected object or ']'"));
        }
        let obj_end = find_matching_brace(slice, i)
            .ok_or_else(|| malformed_array(field_name, "unclosed object"))?;
        let chunk = &slice[i..=obj_end];
        items.push(parse_elem(chunk)?);
        i = obj_end + 1;
    }
    if !closed {
        return Err(malformed_array(
            field_name,
            &format!("truncated {field_name} array"),
        ));
    }
    Ok(items)
}

fn parse_lang_item_field(chunk: &str) -> Option<PxiLangItem> {
    let key = "\"lang_item\"";
    let pos = chunk.find(key)?;
    let after = &chunk[pos + key.len()..];
    let brace = after.find('{')?;
    let obj_end = find_matching_brace(after, brace)?;
    let obj = &after[brace..=obj_end];
    let name = extract_field_string(obj, "name")?;
    let kind = extract_field_string(obj, "kind")?;
    Some(PxiLangItem { name, kind })
}

fn parse_exports(logical_module: &str, text: &str) -> Result<Vec<PxiExport>, PxiError> {
    parse_json_object_array(text, "exports", |chunk| {
        parse_export_object(logical_module, chunk)
    })
}

fn parse_export_object(logical_module: &str, chunk: &str) -> Result<PxiExport, PxiError> {
    let name = extract_field_string(chunk, "name").ok_or_else(|| PxiError::Parse {
        message: "malformed export: missing 'name'".to_owned(),
    })?;
    let kind = extract_field_string(chunk, "kind").ok_or_else(|| PxiError::Parse {
        message: "malformed export: missing 'kind'".to_owned(),
    })?;
    let sig = extract_field_string(chunk, "signature").ok_or_else(|| PxiError::Parse {
        message: "malformed export: missing 'signature'".to_owned(),
    })?;
    let export_id = extract_field_string(chunk, "export_id")
        .unwrap_or_else(|| stable_export_id(logical_module, &name, &kind));
    let ty = parse_export_type(chunk);
    let function_id = extract_field_u32(chunk, "function_id");
    let lang_item = parse_lang_item_field(chunk);
    Ok(PxiExport {
        export_id,
        name,
        kind,
        signature: sig,
        ty,
        function_id,
        lang_item,
    })
}

fn find_matching_brace(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(start)? != &b'{' {
        return None;
    }
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
                continue;
            }
            if b == b'\\' {
                escape = true;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            continue;
        }
        if b == b'"' {
            in_string = true;
            continue;
        }
        if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn parse_dependencies(text: &str) -> Result<Vec<PxiDependency>, PxiError> {
    parse_json_object_array(text, "dependencies", parse_dependency_object)
}

fn parse_dependency_object(chunk: &str) -> Result<PxiDependency, PxiError> {
    let logical_module =
        extract_field_string(chunk, "logical_module").ok_or_else(|| PxiError::Parse {
            message: "malformed dependency: missing 'logical_module'".to_owned(),
        })?;
    let pxi_hash = extract_field_string(chunk, "pxi_hash").ok_or_else(|| PxiError::Parse {
        message: "malformed dependency: missing 'pxi_hash'".to_owned(),
    })?;
    Ok(PxiDependency {
        logical_module,
        pxi_hash,
    })
}

fn parse_export_type(chunk: &str) -> Option<PxiType> {
    let pat = "\"type\":";
    let pos = chunk.find(pat)? + pat.len();
    parse_type_value(&chunk[pos..])
}

fn extract_field_u32(chunk: &str, key: &str) -> Option<u32> {
    let pat = format!("\"{key}\":");
    let pos = chunk.find(&pat)? + pat.len();
    let rest = chunk[pos..].trim_start();
    let num: String = rest.chars().take_while(char::is_ascii_digit).collect();
    num.parse().ok()
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
                function_id: Some(0),
                lang_item: None,
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
        assert_eq!(back.exports[0].function_id, Some(0));
    }

    #[test]
    fn parse_function_id_with_structured_type() {
        let json = r#"{
  "format_version": 2,
  "logical_module": "math",
  "source_hash": "h",
  "origin": null,
  "exports": [
    {"export_id": "math::add::fn", "name": "add", "kind": "fn", "signature": "(S32, S32) => S32", "function_id": 0, "type": { "kind": "fn", "params": [], "ret": { "kind": "primitive", "name": "s32" } }}
  ],
  "dependencies": []
}"#;
        let pxi = PxiFile::parse(json).unwrap();
        assert_eq!(pxi.exports[0].function_id, Some(0));
    }

    #[test]
    fn parse_exports_ignore_nested_type_names() {
        let json = r#"{
  "format_version": 2,
  "logical_module": "math",
  "source_hash": "h",
  "origin": null,
  "exports": [
    {"export_id": "math::add::fn", "name": "add", "kind": "fn", "signature": "(S32, S32) => S32", "function_id": 0, "type": { "kind": "fn", "params": [{ "kind": "primitive", "name": "s32" }], "ret": { "kind": "primitive", "name": "s32" } }},
    {"export_id": "math::id::fn", "name": "id", "kind": "fn", "signature": "(T) => T"}
  ],
  "dependencies": []
}"#;
        let pxi = PxiFile::parse(json).unwrap();
        assert_eq!(pxi.exports.len(), 2);
        let add = pxi.exports.iter().find(|e| e.name == "add").expect("add");
        assert_eq!(add.export_id, "math::add::fn");
        assert_eq!(add.function_id, Some(0));
        let id = pxi.exports.iter().find(|e| e.name == "id").expect("id");
        assert_eq!(id.export_id, "math::id::fn");
        assert_eq!(id.function_id, None);
    }

    fn minimal_pxi_json(exports: &str, dependencies: &str) -> String {
        format!(
            r#"{{
  "format_version": 1,
  "logical_module": "m",
  "source_hash": "h",
  "origin": null,
  "exports": {exports},
  "dependencies": {dependencies}
}}"#
        )
    }

    #[test]
    fn parse_empty_exports_and_dependencies() {
        let json = minimal_pxi_json("[]", "[]");
        let pxi = PxiFile::parse(&json).unwrap();
        assert!(pxi.exports.is_empty());
        assert!(pxi.dependencies.is_empty());
    }

    #[test]
    fn parse_rejects_missing_exports() {
        let json = r#"{
  "format_version": 1,
  "logical_module": "m",
  "source_hash": "h",
  "origin": null,
  "dependencies": []
}"#;
        let err = PxiFile::parse(json).unwrap_err();
        assert_eq!(
            err,
            PxiError::Parse {
                message: "missing exports".to_owned()
            }
        );
    }

    #[test]
    fn parse_rejects_truncated_exports_array() {
        let json = r#"{
  "format_version": 1,
  "logical_module": "m",
  "source_hash": "h",
  "origin": null,
  "exports": [
    {"export_id": "m::add::fn", "name": "add", "kind": "fn", "signature": "() => ()"},
"#;
        let err = PxiFile::parse(json).unwrap_err();
        assert_eq!(
            err,
            PxiError::Parse {
                message: "malformed exports: truncated exports array".to_owned()
            }
        );
    }

    #[test]
    fn parse_rejects_garbage_in_exports_array() {
        let json = minimal_pxi_json("[ 123 ]", "[]");
        let err = PxiFile::parse(&json).unwrap_err();
        assert_eq!(
            err,
            PxiError::Parse {
                message: "malformed exports: expected object or ']'".to_owned()
            }
        );
    }

    #[test]
    fn parse_rejects_unclosed_export_object() {
        let json = r#"{
  "format_version": 1,
  "logical_module": "m",
  "source_hash": "h",
  "origin": null,
  "exports": [{"name": "a", "kind": "fn", "signature": "s", "type": {
  "dependencies": []
}"#;
        let err = PxiFile::parse(json).unwrap_err();
        assert_eq!(
            err,
            PxiError::Parse {
                message: "malformed exports: unclosed object".to_owned()
            }
        );
    }

    #[test]
    fn parse_rejects_export_missing_name() {
        let json = minimal_pxi_json(
            r#"[{"export_id": "m::add::fn", "kind": "fn", "signature": "() => ()"}]"#,
            "[]",
        );
        let err = PxiFile::parse(&json).unwrap_err();
        assert_eq!(
            err,
            PxiError::Parse {
                message: "malformed export: missing 'name'".to_owned()
            }
        );
    }

    #[test]
    fn parse_rejects_export_missing_kind() {
        let json = minimal_pxi_json(
            r#"[{"export_id": "m::add::fn", "name": "add", "signature": "() => ()"}]"#,
            "[]",
        );
        let err = PxiFile::parse(&json).unwrap_err();
        assert_eq!(
            err,
            PxiError::Parse {
                message: "malformed export: missing 'kind'".to_owned()
            }
        );
    }

    #[test]
    fn parse_rejects_export_missing_signature() {
        let json = minimal_pxi_json(
            r#"[{"export_id": "m::add::fn", "name": "add", "kind": "fn"}]"#,
            "[]",
        );
        let err = PxiFile::parse(&json).unwrap_err();
        assert_eq!(
            err,
            PxiError::Parse {
                message: "malformed export: missing 'signature'".to_owned()
            }
        );
    }

    #[test]
    fn parse_rejects_missing_dependencies() {
        let json = r#"{
  "format_version": 1,
  "logical_module": "m",
  "source_hash": "h",
  "origin": null,
  "exports": []
}"#;
        let err = PxiFile::parse(json).unwrap_err();
        assert_eq!(
            err,
            PxiError::Parse {
                message: "missing dependencies".to_owned()
            }
        );
    }

    #[test]
    fn parse_rejects_truncated_dependencies_array() {
        let json = r#"{
  "format_version": 1,
  "logical_module": "m",
  "source_hash": "h",
  "origin": null,
  "exports": [],
  "dependencies": [
    {"logical_module": "core", "pxi_hash": "abc"},
"#;
        let err = PxiFile::parse(json).unwrap_err();
        assert_eq!(
            err,
            PxiError::Parse {
                message: "malformed dependencies: truncated dependencies array".to_owned()
            }
        );
    }

    #[test]
    fn parse_rejects_dependency_missing_pxi_hash() {
        let json = minimal_pxi_json("[]", r#"[{"logical_module": "core"}]"#);
        let err = PxiFile::parse(&json).unwrap_err();
        assert_eq!(
            err,
            PxiError::Parse {
                message: "malformed dependency: missing 'pxi_hash'".to_owned()
            }
        );
    }
}

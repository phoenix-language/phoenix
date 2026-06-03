//! Logical module paths and filesystem mapping.

use std::path::{Path, PathBuf};

use phx_syntax::Interner;
use phx_syntax::ast::ident::{Path as AstPath, PathSegment};

/// Logical module path (`a::b::c`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModulePath {
    segments: Vec<String>,
}

impl ModulePath {
    /// Creates a path from interned segment strings.
    #[must_use]
    pub fn new(segments: Vec<String>) -> Self {
        Self { segments }
    }

    /// Returns path segments.
    #[cfg(test)]
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Renders `a::b::c` for diagnostics.
    #[must_use]
    pub fn display(&self) -> String {
        self.segments.join("::")
    }

    /// Builds a module path from a `.phx` file relative to `module_root`.
    ///
    /// `app/main.phx` under root → `app::main`; `lib/mod.phx` → `lib::mod`.
    #[must_use]
    pub fn from_file_path(module_root: &Path, file: &Path) -> Option<Self> {
        let rel = file.strip_prefix(module_root).ok()?;
        let mut parts: Vec<String> = rel
            .components()
            .filter_map(|c| c.as_os_str().to_str().map(str::to_owned))
            .collect();
        if parts.is_empty() {
            return None;
        }
        if parts.last().is_some_and(|p| p.ends_with(".phx")) {
            let last = parts.pop()?;
            let stem = last.strip_suffix(".phx")?;
            if stem == "index" && !parts.is_empty() {
                // index.phx uses parent folder name only (handled when resolving imports).
            } else if !stem.is_empty() {
                parts.push(stem.to_owned());
            }
        }
        if parts.is_empty() {
            return None;
        }
        Some(Self::new(parts))
    }

    /// Maps an import / qualified path to a filesystem path under `module_root`.
    #[must_use]
    pub fn to_file_path(&self, module_root: &Path) -> PathBuf {
        let mut p = module_root.to_path_buf();
        for seg in &self.segments {
            p.push(seg);
        }
        p.set_extension("phx");
        p
    }

    /// Alternate layout: `path/index.phx`.
    #[must_use]
    pub fn to_index_file_path(&self, module_root: &Path) -> PathBuf {
        let mut p = module_root.to_path_buf();
        for seg in &self.segments {
            p.push(seg);
        }
        p.push("index.phx");
        p
    }

    /// Resolves which file exists for this module path.
    pub fn resolve_existing_file(module_root: &Path, path: &ModulePath) -> Option<PathBuf> {
        let direct = path.to_file_path(module_root);
        if direct.is_file() {
            return Some(direct);
        }
        let index = path.to_index_file_path(module_root);
        if index.is_file() {
            return Some(index);
        }
        None
    }

    /// Extracts module path from an AST path used in `#import` (all segments).
    #[must_use]
    pub fn from_ast_path(path: &AstPath, interner: &Interner) -> Self {
        let segments: Vec<String> = path
            .segments
            .iter()
            .filter_map(|s| segment_to_string(s, interner))
            .collect();
        Self::new(segments)
    }

    /// For `#import a::b::Item` without braces: module `a::b`, item `Item`.
    #[must_use]
    pub fn split_import_target(path: &AstPath, interner: &Interner) -> (Self, String) {
        let segments: Vec<String> = path
            .segments
            .iter()
            .filter_map(|s| segment_to_string(s, interner))
            .collect();
        if segments.len() <= 1 {
            let item = segments.first().cloned().unwrap_or_default();
            return (Self::new(vec![]), item);
        }
        let item = segments.last().cloned().unwrap_or_default();
        let module = Self::new(segments[..segments.len() - 1].to_vec());
        (module, item)
    }
}

fn segment_to_string(seg: &PathSegment, interner: &Interner) -> Option<String> {
    let sym = match seg {
        PathSegment::Ident(i) => i.symbol,
        PathSegment::Type(t) => t.symbol,
    };
    Some(interner.resolve(sym).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_path_to_module_path() {
        let root = Path::new("/proj");
        let file = Path::new("/proj/math/common.phx");
        let mp = ModulePath::from_file_path(root, file).expect("path");
        assert_eq!(mp.segments(), &["math", "common"]);
    }

    #[test]
    fn resolve_direct_or_index() {
        let dir = std::env::temp_dir().join("phx_mod_test_direct");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("a/b/c.phx"), "main :: () => { };").unwrap();
        let mp = ModulePath::new(vec!["a".into(), "b".into(), "c".into()]);
        assert!(ModulePath::resolve_existing_file(&dir, &mp).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

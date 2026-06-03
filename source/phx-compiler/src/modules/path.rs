//! Logical module paths and filesystem mapping.

use std::path::{Path, PathBuf};

use phx_syntax::Interner;
use phx_syntax::ast::ident::{Path as AstPath, PathSegment};

use crate::project::PackageType;

/// Logical module path (`a::b::c`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModulePath {
    segments: Vec<String>,
}

impl ModulePath {
    /// Creates a path from segment strings.
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

    /// Builds a logical path from a `.phx` file under `module_root` for `package_name`.
    ///
    /// Special files: `main.phx` / `lib.phx` at root → `{package}`; `dir/mod.phx` → `{package}::dir`.
    #[must_use]
    pub fn from_file_path(module_root: &Path, file: &Path, package_name: &str) -> Option<Self> {
        let rel = file.strip_prefix(module_root).ok()?;
        let mut parts: Vec<String> = rel
            .components()
            .filter_map(|c| c.as_os_str().to_str().map(str::to_owned))
            .collect();
        if parts.is_empty() {
            return None;
        }
        let stem = parts.pop()?.strip_suffix(".phx")?.to_owned();
        let inner = if parts.is_empty() && (stem == "main" || stem == "lib") {
            Vec::new()
        } else if stem == "mod" {
            parts
        } else {
            parts.push(stem);
            parts
        };
        let mut segments = vec![package_name.to_owned()];
        segments.extend(inner);
        Some(Self::new(segments))
    }

    /// Segments after the package name prefix.
    #[must_use]
    pub fn within_package<'a>(&'a self, package_name: &str) -> &'a [String] {
        if self.segments.first().is_some_and(|s| s == package_name) {
            &self.segments[1..]
        } else {
            &self.segments
        }
    }

    /// Canonicalizes an import path for `workspace_name` and known dependency names.
    #[must_use]
    pub fn canonicalize_import(target: &Self, workspace_name: &str, dep_names: &[&str]) -> Self {
        if target.segments.is_empty() {
            return target.clone();
        }
        let first = &target.segments[0];
        if first == workspace_name || dep_names.iter().any(|d| *d == first) {
            return target.clone();
        }
        let mut segs = vec![workspace_name.to_owned()];
        segs.extend(target.segments.clone());
        Self::new(segs)
    }

    /// Maps a logical path to a filesystem path under `module_root`.
    #[must_use]
    pub fn resolve_existing_file(
        module_root: &Path,
        path: &Self,
        package_name: &str,
        package_type: PackageType,
    ) -> Option<PathBuf> {
        let inner = path.within_package(package_name);
        if inner.is_empty() {
            let main = module_root.join("main.phx");
            let lib = module_root.join("lib.phx");
            return match package_type {
                PackageType::Bin if main.is_file() => Some(main),
                PackageType::Lib if lib.is_file() => Some(lib),
                _ => None,
            };
        }
        if inner.len() == 1 {
            let direct = module_root.join(format!("{}.phx", inner[0]));
            if direct.is_file() {
                return Some(direct);
            }
            let mod_file = module_root.join(&inner[0]).join("mod.phx");
            if mod_file.is_file() {
                return Some(mod_file);
            }
            return None;
        }
        let mut p = module_root.to_path_buf();
        for seg in &inner[..inner.len() - 1] {
            p.push(seg);
        }
        let last = inner.last()?;
        let direct = p.join(format!("{last}.phx"));
        if direct.is_file() {
            return Some(direct);
        }
        let mod_file = p.join(last).join("mod.phx");
        if mod_file.is_file() {
            return Some(mod_file);
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
    fn package_root_main() {
        let root = Path::new("/proj/src");
        let file = Path::new("/proj/src/main.phx");
        let mp = ModulePath::from_file_path(root, file, "myapp").expect("path");
        assert_eq!(mp.segments(), &["myapp"]);
    }

    #[test]
    fn mod_file_collapses() {
        let root = Path::new("/proj/src");
        let file = Path::new("/proj/src/utils/mod.phx");
        let mp = ModulePath::from_file_path(root, file, "math").expect("path");
        assert_eq!(mp.segments(), &["math", "utils"]);
    }

    #[test]
    fn canonicalize_same_package() {
        let t = ModulePath::new(vec!["utils".into(), "math".into()]);
        let c = ModulePath::canonicalize_import(&t, "myapp", &["math"]);
        assert_eq!(c.display(), "myapp::utils::math");
    }
}

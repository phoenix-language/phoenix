//! Logical module paths and filesystem mapping.
//!
//! ## Pass role
//!
//! Bridges Phoenix logical paths (`pkg::a::b`) and on-disk layout (`lib.phx`, `a.phx`, `a/mod.phx`).
//! Used by the module loader ([`super::loader`]), discovery ([`super::discover`]), import resolution
//! ([`super::import_resolve`]), and the dependency graph ([`super::graph`]) to canonicalize `#import`
//! targets, map entry files to logical paths, and locate dependency modules under each package's
//! `module_src` root.
//!
//! ## Inputs and outputs
//!
//! | Function | Reads | Produces |
//! | --- | --- | --- |
//! | [`ModulePath::from_file_path`] | Filesystem path under `module_root` | Logical path for a discovered `.phx` file |
//! | [`ModulePath::from_ast_path`] | `#import` AST path + [`Interner`](phx_syntax::Interner) | Logical path with all segments |
//! | [`ModulePath::split_import_target`] | `#import a::b::Item` AST path | Module path `a::b` and item name `Item` |
//! | [`ModulePath::canonicalize_import`] | Relative import + workspace/dep names | Fully qualified path with package prefix |
//! | [`ModulePath::resolve_existing_file`] | Logical path + `module_root` | Existing `.phx` file on disk, if any |
//!
//! [`ModulePath`] is the shared type across load, discover, and import resolve. Path strings in
//! [`super::loader`] indexes use [`Self::display`] as the canonical key.

use std::path::{Path, PathBuf};

use phx_syntax::Interner;
use phx_syntax::ast::ident::{Path as AstPath, PathSegment};

use crate::project::PackageType;

/// Logical module path (`a::b::c`).
///
/// A sequence of path segments where the first segment is always the **package name** and remaining
/// segments name nested modules within that package. Empty inner segments after the package denote
/// the package root module (`main.phx` for binaries, `lib.phx` for libraries).
///
/// Logical paths are distinct from filesystem paths: the same [`ModulePath`] may map to either
/// `utils.phx` or `utils/mod.phx` depending on what exists under `module_src` (see
/// [`Self::resolve_existing_file`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModulePath {
    segments: Vec<String>,
}

impl ModulePath {
    /// Creates a path from segment strings.
    ///
    /// The caller must supply a non-empty `segments` vector whose first element is the package name
    /// when representing a fully qualified path. Relative import paths may omit the package prefix
    /// until [`Self::canonicalize_import`] runs.
    ///
    /// # Panics
    ///
    /// Never panics.
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

    /// Renders `a::b::c` for diagnostics and path-index keys.
    ///
    /// Joins [`Self::segments`] with `::`. An empty segment list renders an empty string.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn display(&self) -> String {
        self.segments.join("::")
    }

    /// Returns the package name (first path segment).
    ///
    /// Returns an empty string when [`Self::segments`] is empty (relative import before
    /// canonicalization).
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn package_name(&self) -> &str {
        self.segments.first().map_or("", String::as_str)
    }

    /// Builds a logical path from a `.phx` file under `module_root` for `package_name`.
    ///
    /// Strips `module_root` from `file`, removes the `.phx` extension, and applies Phoenix module
    /// layout rules:
    ///
    /// | On-disk path (relative to `module_root`) | Logical path |
    /// | --- | --- |
    /// | `main.phx` or `lib.phx` at root | `{package_name}` |
    /// | `foo.phx` | `{package_name}::foo` |
    /// | `foo/mod.phx` | `{package_name}::foo` |
    /// | `a/b.phx` | `{package_name}::a::b` |
    /// | `a/b/mod.phx` | `{package_name}::a::b` |
    ///
    /// Returns `None` when `file` is not under `module_root`, has no `.phx` extension, or yields
    /// no path components.
    ///
    /// # Panics
    ///
    /// Never panics on malformed paths.
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
    ///
    /// When the first segment equals `package_name`, returns `segments[1..]`. Otherwise returns all
    /// segments unchanged (relative paths not yet canonicalized).
    ///
    /// Used by [`Self::resolve_existing_file`] to map inner module names back to filesystem layout.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn within_package<'a>(&'a self, package_name: &str) -> &'a [String] {
        if self.segments.first().is_some_and(|s| s == package_name) {
            &self.segments[1..]
        } else {
            &self.segments
        }
    }

    /// Canonicalizes an import path for `workspace_name` and known dependency names.
    ///
    /// Relative imports (first segment is neither the workspace package nor a path dependency) are
    /// prefixed with `workspace_name`. Already-qualified paths whose first segment matches
    /// `workspace_name` or any name in `dep_names` are returned unchanged.
    ///
    /// Example: with workspace `myapp` and dependency `math`, import `utils::foo` becomes
    /// `myapp::utils::foo`, while `math::bar` stays `math::bar`.
    ///
    /// # Panics
    ///
    /// Never panics.
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

    /// Maps a logical path to an existing filesystem path under `module_root`.
    ///
    /// Only returns paths that **exist** on disk at resolve time. Resolution order for each module
    /// segment prefers a flat file over a directory module:
    ///
    /// - **Package root** (`inner` empty): `main.phx` for [`PackageType::Bin`], `lib.phx` for
    ///   [`PackageType::Lib`].
    /// - **Single inner segment** `foo`: `foo.phx`, then `foo/mod.phx`.
    /// - **Nested** `a::b`: `a/b.phx`, then `a/b/mod.phx`.
    ///
    /// Returns `None` when no matching file exists or the path does not belong to `package_name`.
    ///
    /// # Panics
    ///
    /// Never panics on missing directories.
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
    ///
    /// Converts each [`PathSegment`] to a display string via `interner`. Use
    /// [`Self::split_import_target`] when the last segment names an imported item rather than a
    /// module.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn from_ast_path(path: &AstPath, interner: &Interner) -> Self {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|s| segment_to_string(s, interner))
            .collect();
        Self::new(segments)
    }

    /// For `#import a::b::Item` without braces: module `a::b`, item `Item`.
    ///
    /// When the path has at most one segment, the entire path is treated as the item name and the
    /// module path is empty (same-package single-segment import). Brace imports (`#import a::b::{…}`)
    /// use [`Self::from_ast_path`] instead because every segment belongs to the module path.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn split_import_target(path: &AstPath, interner: &Interner) -> (Self, String) {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|s| segment_to_string(s, interner))
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

/// Resolves one AST path segment to its source spelling.
fn segment_to_string(seg: &PathSegment, interner: &Interner) -> String {
    let sym = match seg {
        PathSegment::Ident(i) => i.symbol,
        PathSegment::Type(t) => t.name.symbol,
    };
    interner.resolve_display(sym)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
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

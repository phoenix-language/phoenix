//! `phoenix.toml` parsing and project schema validation (M2).
//!
//! Loads the minimal TOML subset Phoenix supports today, optionally injects the bundled
//! [`std`](super::stdlib) library, and validates filesystem layout before
//! [`crate::build`] runs.
//!
//! ## Supported TOML sections
//!
//! | Section | Keys | Notes |
//! | --- | --- | --- |
//! | `[project]` | `name`, `type`, `module_src`, `version`, `description`, `edition`, `module_roots`, `bundle_std`, `prelude` | `name` and `type` are required |
//! | `[build]` | `dir` | Defaults to `build` |
//! | `[vm]` | `heap_cap` | Integer or suffix (`"64mb"`); applied at `phx run` |
//! | `[lint]` | `deny` | Lint kinds that fail `phx check` / `phx build` |
//! | `[dependencies.{name}]` | `path` | Inline `{ path = "..." }` also accepted |
//!
//! Unknown keys and sections are ignored. Comments (`#`) and blank lines are stripped per line.
//!
//! ## Load and validation flow
//!
//! ```text
//! read phoenix.toml → parse_toml → apply_bundled_std (optional) → validate → ProjectConfig
//! ```
//!
//! [`ProjectConfig::validate`] runs after parse (and after std injection for [`ProjectConfig::load`]):
//!
//! - `module_src` must exist as a directory under [`ProjectConfig::root`].
//! - [`ProjectConfig::default_entry_file`] must exist (`main.phx` for `bin`, `lib.phx` for `lib`).
//! - Each `[dependencies]` entry must point at a directory with a valid `phoenix.toml` whose
//!   `project.name` matches the dependency key and `project.type` is `lib`.
//!
//! ## Bundled std
//!
//! [`ProjectConfig::load`] calls [`super::stdlib::apply_bundled_std`] unless
//! `bundle_std = false` or `std` is already declared. [`ProjectConfig::load_without_bundled_std`]
//! skips injection (used when validating the `std` package root to avoid recursion).
//!
//! ## Entry points
//!
//! - [`ProjectConfig::load`] — primary CLI/embedder loader (parse + std + validate)
//! - [`ProjectConfig::load_without_bundled_std`] — same without std injection
//! - [`ProjectConfig::module_root`] / [`ProjectConfig::build_root`] — absolute path helpers
//! - [`ProjectConfig::default_entry_file`] — `main.phx` or `lib.phx` under `module_src`
//! - [`ProjectConfig::output_name`] — linked artifact stem (`project.name`)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use phx_diagnostics::LintDenyConfig;

use crate::byte_size;

/// Package kind from `[project] type`.
///
/// Controls the required entry file, link output directory ([`BuildLayout`](super::BuildLayout)),
/// and type-check rules for the crate root (`main` function required vs forbidden).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageType {
    /// Executable crate (`type = "bin"`).
    ///
    /// Requires `main.phx` at [`ProjectConfig::default_entry_file`] with a `main` entry point.
    /// Linked output is written under `build/bin/{name}.phx0`.
    Bin,
    /// Library crate (`type = "lib"`).
    ///
    /// Requires `lib.phx` at [`ProjectConfig::default_entry_file`]; a `main` function is
    /// forbidden. Linked output is written under `build/lib/{name}.phx0`. Path dependencies
    /// must use this variant ([`ProjectConfig::validate`] rejects `bin` deps).
    Lib,
}

/// Path dependency entry from `[dependencies.{name}]`.
///
/// Parsed from either `name = { path = "relative/or/absolute" }` or a
/// `[dependencies.name]` subsection with `path = "..."`. The path is stored relative to
/// [`ProjectConfig::root`] when possible; resolution uses `root.join(path)` at build time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathDependency {
    /// Filesystem path to the dependency project root (contains `phoenix.toml`).
    pub path: PathBuf,
}

/// Parsed `phoenix.toml` project configuration.
///
/// Produced by [`ProjectConfig::load`] or [`ProjectConfig::load_without_bundled_std`] after
/// parse, optional bundled-std injection, and [`ProjectConfig::validate`]. Immutable for the
/// duration of a build; path fields are relative to [`Self::root`] unless documented otherwise.
///
/// Consumed by [`crate::build::build_project`], [`crate::modules::ProgramLoadContext`], and
/// [`BuildLayout`](super::BuildLayout) to locate sources and write artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectConfig {
    /// Project root directory (directory containing `phoenix.toml`).
    pub root: PathBuf,
    /// `[project] name` — package name, logical module namespace root, and linked output stem.
    pub name: String,
    /// `[project] version` — metadata only in MVP (not enforced at link time).
    pub version: String,
    /// `[project] description` — metadata only in MVP.
    pub description: String,
    /// `[project] edition` — reserved for future edition gating; not enforced in MVP.
    pub edition: String,
    /// `[project] module_roots` — reserved; MVP uses [`Self::module_src`] only.
    pub module_roots: Vec<PathBuf>,
    /// `[project] type` — [`PackageType::Bin`] or [`PackageType::Lib`].
    pub package_type: PackageType,
    /// `[project] module_src` — relative path to Phoenix source root (default `src`).
    pub module_src: PathBuf,
    /// `[build] dir` — relative path to build output root (default `build`).
    pub build_dir: PathBuf,
    /// `[dependencies]` keyed by package name; each key must match the depended crate's `name`.
    pub dependencies: HashMap<String, PathDependency>,
    /// When `true` (default), [`super::stdlib::apply_bundled_std`] adds a `std` path dep unless already declared.
    pub bundle_std: bool,
    /// When `true` (default), inject std prelude bindings when the `std` package is linked.
    pub prelude: bool,
    /// `[vm] heap_cap` — VM linear heap byte cap for `phx run`; `None` → 64 MiB default at run.
    pub vm_heap_cap_bytes: Option<usize>,
    /// `[lint] deny` — lint warnings promoted to errors during `phx check` / `phx build` (CLI `--deny` overrides).
    pub lint_deny: LintDenyConfig,
}

impl ProjectConfig {
    /// Loads `phoenix.toml` from `root`.
    ///
    /// Reads `root/phoenix.toml`, parses the supported TOML subset, injects the bundled
    /// `std` dependency when [`Self::bundle_std`] applies, then runs [`Self::validate`].
    /// This is the loader used by [`super::discover::discover_project`] and
    /// [`crate::build::build_project`].
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::Io`] when `phoenix.toml` cannot be read.
    /// Returns [`ProjectError::Invalid`] for parse failures (missing `name`/`type`, bad
    /// booleans, invalid `vm.heap_cap` or `lint.deny`), std resolution failures from
    /// [`super::stdlib::apply_bundled_std`], or [`Self::validate`] violations.
    ///
    /// # Panics
    ///
    /// Never panics on user-supplied paths or malformed TOML.
    pub fn load(root: &Path) -> Result<Self, ProjectError> {
        let path = root.join("phoenix.toml");
        let text = std::fs::read_to_string(&path).map_err(|e| ProjectError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        let mut cfg = parse_toml(&text, root)?;
        super::stdlib::apply_bundled_std(&mut cfg)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Loads `phoenix.toml` without injecting the bundled `std` dependency.
    ///
    /// Same as [`Self::load`] except [`super::stdlib::apply_bundled_std`] is skipped.
    /// Used when validating the `std` package root ([`Self::name`] == `"std"`) to avoid
    /// recursive bundling, and by tests that declare dependencies explicitly.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::Io`] when `phoenix.toml` cannot be read.
    /// Returns [`ProjectError::Invalid`] for parse or [`Self::validate`] failures.
    ///
    /// # Panics
    ///
    /// Never panics on user-supplied paths or malformed TOML.
    pub fn load_without_bundled_std(root: &Path) -> Result<Self, ProjectError> {
        let path = root.join("phoenix.toml");
        let text = std::fs::read_to_string(&path).map_err(|e| ProjectError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        let cfg = parse_toml(&text, root)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Absolute path to the module source root (`root` + [`Self::module_src`]).
    #[must_use]
    pub fn module_root(&self) -> PathBuf {
        self.root.join(&self.module_src)
    }

    /// Absolute path to the build directory (`root` + [`Self::build_dir`]).
    #[must_use]
    pub fn build_root(&self) -> PathBuf {
        self.root.join(&self.build_dir)
    }

    /// Default entry source file for this package.
    ///
    /// Returns `module_root/main.phx` for [`PackageType::Bin`] or `module_root/lib.phx`
    /// for [`PackageType::Lib`]. The CLI uses this when no explicit entry path is given;
    /// [`Self::validate`] requires the file to exist.
    #[must_use]
    pub fn default_entry_file(&self) -> PathBuf {
        match self.package_type {
            PackageType::Bin => self.module_root().join("main.phx"),
            PackageType::Lib => self.module_root().join("lib.phx"),
        }
    }

    /// Linked artifact file name stem (`[project] name`).
    ///
    /// The build driver writes `build/bin/{output_name}.phx0` or
    /// `build/lib/{output_name}.phx0` depending on [`Self::package_type`].
    #[must_use]
    pub fn output_name(&self) -> &str {
        &self.name
    }

    /// Validates filesystem layout, entry files, and path dependencies.
    ///
    /// Checks run in order: `module_src` directory exists, default entry file exists,
    /// then each dependency's `phoenix.toml` is loaded recursively (via [`Self::load`]) and
    /// checked for `type = lib` and a matching `project.name`.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::Invalid`] when `module_src` is missing, the required
    /// `main.phx`/`lib.phx` is absent, a dependency path has no `phoenix.toml`, a
    /// dependency is not `lib`, or a dependency key does not match `project.name`.
    /// Propagates [`ProjectError::Io`] and nested [`ProjectError::Invalid`] from
    /// dependency [`Self::load`] calls.
    ///
    /// # Panics
    ///
    /// Never panics on user-supplied configuration.
    pub fn validate(&self) -> Result<(), ProjectError> {
        let module_root = self.module_root();
        if !module_root.is_dir() {
            return Err(ProjectError::Invalid {
                message: format!(
                    "module_src `{}` is not a directory",
                    self.module_src.display()
                ),
            });
        }
        let entry = self.default_entry_file();
        if !entry.is_file() {
            let want = match self.package_type {
                PackageType::Bin => "main.phx",
                PackageType::Lib => "lib.phx",
            };
            return Err(ProjectError::Invalid {
                message: format!(
                    "missing required `{want}` at `{}` for type {:?}",
                    entry.display(),
                    self.package_type
                ),
            });
        }
        for (key, dep) in &self.dependencies {
            let dep_root = self.root.join(&dep.path);
            if !dep_root.join("phoenix.toml").is_file() {
                return Err(ProjectError::Invalid {
                    message: format!(
                        "dependency `{key}` path `{}` has no phoenix.toml",
                        dep.path.display()
                    ),
                });
            }
            let dep_cfg = ProjectConfig::load(&dep_root)?;
            if dep_cfg.package_type != PackageType::Lib {
                return Err(ProjectError::Invalid {
                    message: format!("dependency `{key}` must have type = lib"),
                });
            }
            if key != &dep_cfg.name {
                return Err(ProjectError::Invalid {
                    message: format!(
                        "dependency key `{key}` must match dependency project.name `{}`",
                        dep_cfg.name
                    ),
                });
            }
        }
        Ok(())
    }
}

/// Project configuration errors from discovery, load, and validation.
///
/// Surfaced by [`ProjectConfig::load`], [`super::discover::discover_project`], and
/// [`crate::build::BuildError::Project`] when project metadata is missing or invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    /// `phoenix.toml` not found after walking parent directories.
    NotFound {
        /// Starting path passed to [`super::discover::discover_project`].
        from: PathBuf,
    },
    /// Filesystem read failure (missing `phoenix.toml` at an explicit root, permission denied, etc.).
    Io {
        /// Path involved in the I/O operation.
        path: String,
        /// OS error message.
        message: String,
    },
    /// Parse failure, schema violation, or validation rule breach.
    Invalid {
        /// Human-readable reason (included in [`std::fmt::Display`] output).
        message: String,
    },
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { from } => {
                write!(
                    f,
                    "phoenix.toml not found (searched from {})",
                    from.display()
                )
            }
            Self::Io { path, message } => write!(f, "I/O error reading {path}: {message}"),
            Self::Invalid { message } => write!(f, "invalid phoenix.toml: {message}"),
        }
    }
}

impl std::error::Error for ProjectError {}

#[allow(clippy::too_many_lines)]
fn parse_toml(text: &str, root: &Path) -> Result<ProjectConfig, ProjectError> {
    let mut name: Option<String> = None;
    let mut version = "0.0.0".to_owned();
    let mut description = String::new();
    let mut edition = String::new();
    let mut module_roots: Vec<PathBuf> = Vec::new();
    let mut package_type: Option<PackageType> = None;
    let mut module_src = PathBuf::from("src");
    let mut build_dir = PathBuf::from("build");
    let mut section = String::new();
    let mut dep_key: Option<String> = None;
    let mut dependencies: HashMap<String, PathDependency> = HashMap::new();
    let mut bundle_std = true;
    let mut prelude = true;
    let mut vm_heap_cap_bytes: Option<usize> = None;
    let mut lint_deny = LintDenyConfig::warn_only();

    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let inner = line[1..line.len() - 1].trim();
            if let Some(key) = inner.strip_prefix("dependencies.") {
                dep_key = Some(key.to_owned());
                "dependencies".clone_into(&mut section);
            } else {
                dep_key = None;
                inner.clone_into(&mut section);
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = trim_quotes(value.trim());
        match section.as_str() {
            "project" => match key {
                "name" => name = Some(value.to_owned()),
                "version" => value.clone_into(&mut version),
                "description" => value.clone_into(&mut description),
                "edition" => value.clone_into(&mut edition),
                "module_roots" => module_roots.push(PathBuf::from(value)),
                "type" => {
                    package_type = Some(parse_package_type(value)?);
                }
                "module_src" => module_src = PathBuf::from(value),
                "bundle_std" => bundle_std = parse_bool(value)?,
                "prelude" => prelude = parse_bool(value)?,
                _ => {}
            },
            "build" if key == "dir" => build_dir = PathBuf::from(value),
            "vm" if key == "heap_cap" => {
                vm_heap_cap_bytes = Some(parse_heap_cap_value(value)?);
            }
            "lint" if key == "deny" => {
                lint_deny = LintDenyConfig::parse_project(value).map_err(|message| {
                    ProjectError::Invalid {
                        message: format!("invalid lint.deny: {message}"),
                    }
                })?;
            }
            "dependencies" => {
                if key == "path" {
                    let Some(ref dk) = dep_key else {
                        continue;
                    };
                    dependencies.insert(
                        dk.clone(),
                        PathDependency {
                            path: PathBuf::from(value),
                        },
                    );
                } else if value.starts_with('{') {
                    let dep_name = key.to_owned();
                    let path = extract_brace_path(value);
                    if let Some(path) = path {
                        dependencies.insert(
                            dep_name,
                            PathDependency {
                                path: PathBuf::from(path),
                            },
                        );
                    }
                }
            }
            _ => {}
        }
    }

    let name = name.ok_or_else(|| ProjectError::Invalid {
        message: "missing [project] name".to_owned(),
    })?;
    let package_type = package_type.ok_or_else(|| ProjectError::Invalid {
        message: "missing [project] type (bin or lib)".to_owned(),
    })?;

    Ok(ProjectConfig {
        root: root.to_path_buf(),
        name,
        version,
        description,
        edition,
        module_roots,
        package_type,
        module_src,
        build_dir,
        dependencies,
        bundle_std,
        prelude,
        vm_heap_cap_bytes,
        lint_deny,
    })
}

fn parse_heap_cap_value(value: &str) -> Result<usize, ProjectError> {
    byte_size::parse_byte_size(value).map_err(|e| ProjectError::Invalid {
        message: format!("invalid vm.heap_cap: {e}"),
    })
}

fn parse_bool(value: &str) -> Result<bool, ProjectError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(ProjectError::Invalid {
            message: format!("invalid boolean `{other}` (expected true or false)"),
        }),
    }
}

fn parse_package_type(value: &str) -> Result<PackageType, ProjectError> {
    match value {
        "bin" => Ok(PackageType::Bin),
        "lib" => Ok(PackageType::Lib),
        other => Err(ProjectError::Invalid {
            message: format!("invalid project.type `{other}` (expected bin or lib)"),
        }),
    }
}

fn extract_brace_path(value: &str) -> Option<&str> {
    let inner = value.trim().strip_prefix('{')?.strip_suffix('}')?.trim();
    for part in inner.split(',') {
        let part = part.trim();
        let (k, v) = part.split_once('=')?;
        if k.trim() == "path" {
            return Some(trim_quotes(v.trim()));
        }
    }
    None
}

fn trim_quotes(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(s)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn temp_project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("phx_cfg_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn parse_minimal_bin() {
        let dir = temp_project("bin");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("phoenix.toml"),
            r#"
[project]
name = "demo"
type = "bin"
module_src = "src"
bundle_std = false
"#,
        )
        .unwrap();
        std::fs::write(dir.join("src/main.phx"), "main :: () => { };").unwrap();
        let cfg = ProjectConfig::load(&dir).unwrap();
        assert_eq!(cfg.name, "demo");
        assert_eq!(cfg.module_src, PathBuf::from("src"));
        assert_eq!(cfg.package_type, PackageType::Bin);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_minimal_lib() {
        let dir = temp_project("lib");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("phoenix.toml"),
            r#"
[project]
name = "mylib"
type = "lib"
module_src = "src"
bundle_std = false
"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("src/lib.phx"),
            "pub add :: (a: s32, b: s32) => s32 { a + b };",
        )
        .unwrap();
        let cfg = ProjectConfig::load(&dir).unwrap();
        assert_eq!(cfg.name, "mylib");
        assert_eq!(cfg.package_type, PackageType::Lib);
        assert_eq!(cfg.default_entry_file(), dir.join("src/lib.phx"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reject_missing_entry_file() {
        let dir = temp_project("missing_main");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("phoenix.toml"),
            r#"
[project]
name = "demo"
type = "bin"
module_src = "src"
bundle_std = false
"#,
        )
        .unwrap();
        let err = ProjectConfig::load(&dir).unwrap_err();
        assert!(
            matches!(err, ProjectError::Invalid { .. }),
            "expected invalid config, got {err:?}"
        );
        assert!(err.to_string().contains("main.phx"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reject_dep_key_mismatch() {
        let lib_dir = temp_project("dep_lib");
        std::fs::create_dir_all(lib_dir.join("src")).unwrap();
        std::fs::write(
            lib_dir.join("phoenix.toml"),
            r#"
[project]
name = "math"
type = "lib"
module_src = "src"
bundle_std = false
"#,
        )
        .unwrap();
        std::fs::write(
            lib_dir.join("src/lib.phx"),
            "pub add :: (a: s32, b: s32) => s32 { a + b };",
        )
        .unwrap();

        let app_dir = temp_project("dep_app");
        std::fs::create_dir_all(app_dir.join("src")).unwrap();
        std::fs::write(
            app_dir.join("phoenix.toml"),
            format!(
                r#"
[project]
name = "app"
type = "bin"
module_src = "src"
bundle_std = false

[dependencies]
wrong = {{ path = "{}" }}
"#,
                lib_dir.display()
            ),
        )
        .unwrap();
        std::fs::write(app_dir.join("src/main.phx"), "main :: () => { };").unwrap();

        let err = ProjectConfig::load(&app_dir).unwrap_err();
        assert!(
            matches!(err, ProjectError::Invalid { .. }),
            "expected invalid config, got {err:?}"
        );
        assert!(err.to_string().contains("dependency key"));

        let _ = std::fs::remove_dir_all(&lib_dir);
        let _ = std::fs::remove_dir_all(&app_dir);
    }

    #[test]
    fn parse_vm_heap_cap_integer() {
        let dir = temp_project("vm_cap_int");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("phoenix.toml"),
            r#"
[project]
name = "demo"
type = "bin"
module_src = "src"
bundle_std = false

[vm]
heap_cap = 32
"#,
        )
        .unwrap();
        std::fs::write(dir.join("src/main.phx"), "main :: () => { };").unwrap();
        let cfg = ProjectConfig::load(&dir).unwrap();
        assert_eq!(cfg.vm_heap_cap_bytes, Some(32));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_vm_heap_cap_suffix() {
        let dir = temp_project("vm_cap_suffix");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("phoenix.toml"),
            r#"
[project]
name = "demo"
type = "bin"
module_src = "src"
bundle_std = false

[vm]
heap_cap = "64mb"
"#,
        )
        .unwrap();
        std::fs::write(dir.join("src/main.phx"), "main :: () => { };").unwrap();
        let cfg = ProjectConfig::load(&dir).unwrap();
        assert_eq!(cfg.vm_heap_cap_bytes, Some(64 * 1024 * 1024));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_lint_deny_list() {
        let dir = temp_project("lint_deny");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("phoenix.toml"),
            r#"
[project]
name = "demo"
type = "bin"
module_src = "src"
bundle_std = false

[lint]
deny = ["deprecated"]
"#,
        )
        .unwrap();
        std::fs::write(dir.join("src/main.phx"), "main :: () => { };").unwrap();
        let cfg = ProjectConfig::load(&dir).unwrap();
        assert!(cfg.lint_deny.denies(phx_diagnostics::LintKind::Deprecated));
        assert!(!cfg.lint_deny.denies(phx_diagnostics::LintKind::MustUse));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reject_invalid_vm_heap_cap() {
        let dir = temp_project("vm_cap_bad");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("phoenix.toml"),
            r#"
[project]
name = "demo"
type = "bin"
module_src = "src"
bundle_std = false

[vm]
heap_cap = "foo"
"#,
        )
        .unwrap();
        std::fs::write(dir.join("src/main.phx"), "main :: () => { };").unwrap();
        let err = ProjectConfig::load(&dir).unwrap_err();
        assert!(
            matches!(err, ProjectError::Invalid { .. }),
            "expected invalid config, got {err:?}"
        );
        assert!(err.to_string().contains("vm.heap_cap"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

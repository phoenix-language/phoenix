//! Minimal `phoenix.toml` parsing (stdlib only).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Package kind from `[project] type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageType {
    /// Executable; requires `main.phx` and `main` function.
    Bin,
    /// Library; requires `lib.phx`; `main` function forbidden.
    Lib,
}

/// Path dependency entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathDependency {
    /// Filesystem path (relative to project root).
    pub path: PathBuf,
}

/// Parsed `phoenix.toml` project configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectConfig {
    /// Project root directory (contains `phoenix.toml`).
    pub root: PathBuf,
    /// `[project] name` — package name and namespace root.
    pub name: String,
    /// `[project] version`
    pub version: String,
    /// `[project] description`
    pub description: String,
    /// `[project] edition` (reserved; not enforced in MVP).
    pub edition: String,
    /// `[project] module_roots` (reserved; MVP uses `module_src` only).
    pub module_roots: Vec<PathBuf>,
    /// `[project] type`
    pub package_type: PackageType,
    /// `[project] module_src` — source root for modules.
    pub module_src: PathBuf,
    /// `[build] dir`
    pub build_dir: PathBuf,
    /// `[dependencies]` keyed by package name (must match depended `project.name`).
    pub dependencies: HashMap<String, PathDependency>,
    /// When true (default), link the compiler-bundled `std` package unless declared in dependencies.
    pub bundle_std: bool,
    /// When true (default), inject std prelude bindings when std is linked.
    pub prelude: bool,
}

impl ProjectConfig {
    /// Loads `phoenix.toml` from `root`.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the file is missing or invalid.
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
    /// Used when validating the std package root to avoid recursive bundling.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the file is missing or invalid.
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

    /// Absolute path to the module source root.
    #[must_use]
    pub fn module_root(&self) -> PathBuf {
        self.root.join(&self.module_src)
    }

    /// Absolute path to the build directory.
    #[must_use]
    pub fn build_root(&self) -> PathBuf {
        self.root.join(&self.build_dir)
    }

    /// Default entry source file for this package (`main.phx` or `lib.phx`).
    #[must_use]
    pub fn default_entry_file(&self) -> PathBuf {
        match self.package_type {
            PackageType::Bin => self.module_root().join("main.phx"),
            PackageType::Lib => self.module_root().join("lib.phx"),
        }
    }

    /// Linked artifact file name stem (`project.name`).
    #[must_use]
    pub fn output_name(&self) -> &str {
        &self.name
    }

    /// Validates paths, required root files, and dependency keys.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::Invalid`] on schema violations.
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

/// Project configuration errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    /// `phoenix.toml` not found (walk exhausted).
    NotFound {
        /// Directory searched from.
        from: PathBuf,
    },
    /// I/O failure.
    Io {
        /// Path involved.
        path: String,
        /// OS message.
        message: String,
    },
    /// Parse or schema error.
    Invalid {
        /// Human-readable reason.
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
}

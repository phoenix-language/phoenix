//! Minimal `phoenix.toml` parsing (stdlib only).

use std::path::{Path, PathBuf};

/// Parsed `phoenix.toml` project configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectConfig {
    /// Project root directory (contains `phoenix.toml`).
    pub root: PathBuf,
    /// `[project] name`
    pub name: String,
    /// `[project] module_path` — source root for modules.
    pub module_path: PathBuf,
    /// `[build] dir`
    pub build_dir: PathBuf,
    /// `[build] entry` — logical module path (optional).
    pub entry_logical: Option<String>,
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
        parse_toml(&text, root)
    }

    /// Absolute path to the module source root.
    #[must_use]
    pub fn module_root(&self) -> PathBuf {
        self.root.join(&self.module_path)
    }

    /// Absolute path to the build directory.
    #[must_use]
    pub fn build_root(&self) -> PathBuf {
        self.root.join(&self.build_dir)
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
                write!(f, "phoenix.toml not found (searched from {})", from.display())
            }
            Self::Io { path, message } => write!(f, "I/O error reading {path}: {message}"),
            Self::Invalid { message } => write!(f, "invalid phoenix.toml: {message}"),
        }
    }
}

impl std::error::Error for ProjectError {}

fn parse_toml(text: &str, root: &Path) -> Result<ProjectConfig, ProjectError> {
    let mut name: Option<String> = None;
    let mut module_path = PathBuf::from(".");
    let mut build_dir = PathBuf::from("build");
    let mut entry_logical: Option<String> = None;
    let mut section = String::new();

    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_owned();
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
                "module_path" => module_path = PathBuf::from(value),
                _ => {}
            },
            "build" => match key {
                "dir" => build_dir = PathBuf::from(value),
                "entry" => entry_logical = Some(value.replace('/', "::")),
                _ => {}
            },
            _ => {}
        }
    }

    let name = name.ok_or_else(|| ProjectError::Invalid {
        message: "missing [project] name".to_owned(),
    })?;

    Ok(ProjectConfig {
        root: root.to_path_buf(),
        name,
        module_path,
        build_dir,
        entry_logical,
    })
}

fn trim_quotes(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal() {
        let text = r#"
[project]
name = "demo"
module_path = "src"

[build]
dir = "build"
entry = "app/main"
"#;
        let cfg = parse_toml(text, Path::new("/proj")).unwrap();
        assert_eq!(cfg.name, "demo");
        assert_eq!(cfg.module_path, PathBuf::from("src"));
        assert_eq!(cfg.entry_logical.as_deref(), Some("app::main"));
    }
}

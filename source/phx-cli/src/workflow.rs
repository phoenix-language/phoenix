//! Project vs standalone workflow resolution.
//!
//! This module is the **second stage** of the CLI pipeline: after
//! [`crate::args`] produces a parsed subcommand, these functions decide whether
//! the invocation runs in **project mode** (a `phoenix.toml` is discovered) or
//! **standalone mode** (a single entry file with explicit module flags).
//!
//! ```text
//! (CliOptions, Command)
//!       │
//!       ▼
//! resolve_check_mode / resolve_run_mode / resolve_build_project
//!       │
//!       ├──► CompileMode::Project { config }
//!       └──► CompileMode::Standalone { options }
//!       │
//!       ▼
//! crate::commands (compiler / VM)
//!       │
//!       ▼
//! crate::exit::CliExit
//! ```
//!
//! Project discovery honors an explicit `--project-root` when provided;
//! otherwise it walks upward from the entry file or current directory.
//! Violations (file outside module root, standalone file in a project tree)
//! become [`WorkflowError`] and map to [`crate::exit::CliExit::Usage`] in
//! command handlers.
//!
//! ## Public types
//!
//! - [`CompileMode`] — project or standalone compile configuration.
//! - [`WorkflowError`] — project load failure or user-facing rule violation.
//!
//! ## Entry points
//!
//! - [`resolve_check_mode`] — for `phx check <file>`.
//! - [`resolve_run_mode`] — for `phx run` (optional file in project mode).
//! - [`resolve_build_project`] — for `phx build`.

use std::path::Path;
use std::sync::Arc;

use phx_compiler::{
    ProjectConfig, ProjectError, StandaloneOptions, discover_project, resolve_project,
};

use crate::args::FileCommandArgs;

/// How `check`, `compile`, and `run` should invoke the compiler.
///
/// Project mode loads a shared [`ProjectConfig`] from `phoenix.toml`.
/// Standalone mode builds a [`StandaloneOptions`] from CLI file flags when no
/// project manifest is found.
#[derive(Debug)]
pub enum CompileMode {
    /// Full project workflow (`phoenix.toml` discovered).
    Project {
        /// Loaded project configuration shared across the build graph.
        config: Arc<ProjectConfig>,
    },
    /// Single entry file without a project manifest.
    Standalone {
        /// Standalone compile options derived from [`FileCommandArgs`].
        options: StandaloneOptions,
    },
}

/// Workflow resolution failure.
///
/// Either a project configuration error from `phx-compiler` or a CLI rule
/// violation (wrong entry file, file outside module root). Displayed to the
/// user via [`crate::report::Reporter::usage_error`] or
/// [`crate::report::Reporter::project_error`].
#[derive(Debug)]
pub enum WorkflowError {
    /// Project configuration error.
    Project(ProjectError),
    /// User-facing workflow violation.
    Message(String),
}

impl std::fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Project(e) => write!(f, "{e}"),
            Self::Message(m) => f.write_str(m),
        }
    }
}

/// Resolves compile mode for `phx check <file>`.
///
/// When `phoenix.toml` is found, validates that `file` lies under the project
/// module root and returns [`CompileMode::Project`]. Otherwise builds standalone
/// options from `file_args` (module root defaults to the entry file's parent).
///
/// # Errors
///
/// Returns [`WorkflowError::Project`] when the manifest exists but fails to load.
/// Returns [`WorkflowError::Message`] when the entry file is outside the project
/// module root.
pub fn resolve_check_mode(
    file: &Path,
    file_args: &FileCommandArgs,
    project_root: Option<&Path>,
) -> Result<CompileMode, WorkflowError> {
    if let Ok(config) = try_discover_project(file, project_root) {
        validate_file_in_project(file, &config)?;
        return Ok(CompileMode::Project {
            config: Arc::new(config),
        });
    }
    let options = standalone_options(file, file_args)?;
    Ok(CompileMode::Standalone { options })
}

/// Resolves compile mode for `phx run`.
///
/// In a project directory, `file` may be omitted (runs the default entry) or
/// must match the default entry exactly — arbitrary standalone files are
/// rejected. Outside a project, `file` is required and standalone options are
/// built from `file_args`.
///
/// # Errors
///
/// Returns [`WorkflowError::Project`] when the manifest exists but fails to load.
/// Returns [`WorkflowError::Message`] when a non-default file is passed inside
/// a project tree, or when no file is given and no project is found.
pub fn resolve_run_mode(
    file: Option<&Path>,
    file_args: &FileCommandArgs,
    project_root: Option<&Path>,
) -> Result<CompileMode, WorkflowError> {
    if let Ok(config) = try_discover_project(file.unwrap_or_else(|| Path::new(".")), project_root) {
        if let Some(entry) = file {
            let default_entry = config.default_entry_file();
            let canonical_entry = entry.canonicalize().unwrap_or_else(|_| entry.to_path_buf());
            let canonical_default = default_entry.canonicalize().unwrap_or(default_entry);
            if canonical_entry != canonical_default {
                return Err(WorkflowError::Message(format!(
                    "found phoenix.toml at {}; standalone file execution is not allowed in a project directory.\n\
                     Use `phx run` (no file) to run the project entry `{}`, or `phx build`.",
                    config.root.display(),
                    canonical_default.display()
                )));
            }
        }
        return Ok(CompileMode::Project {
            config: Arc::new(config),
        });
    }
    let entry = file.ok_or_else(|| {
        WorkflowError::Message(
            "missing required argument <file.phx> (no phoenix.toml found)".to_owned(),
        )
    })?;
    let options = standalone_options(entry, file_args)?;
    Ok(CompileMode::Standalone { options })
}

/// Resolves a project for `phx build`.
///
/// Locates `phoenix.toml` from `anchor` or an explicit `project_root`. Unlike
/// check/run, build always requires a project — there is no standalone path.
///
/// # Errors
///
/// Returns [`WorkflowError::Project`] when no manifest is found or loading fails.
pub fn resolve_build_project(
    anchor: &Path,
    project_root: Option<&Path>,
) -> Result<ProjectConfig, WorkflowError> {
    resolve_project(anchor, project_root).map_err(WorkflowError::Project)
}

fn try_discover_project(
    start: &Path,
    project_root: Option<&Path>,
) -> Result<ProjectConfig, ProjectError> {
    if let Some(root) = project_root {
        return ProjectConfig::load(root);
    }
    discover_project(start)
}

fn validate_file_in_project(file: &Path, config: &ProjectConfig) -> Result<(), WorkflowError> {
    let module_root = config.module_root();
    let canonical_file = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    let canonical_root = module_root.canonicalize().unwrap_or(module_root);
    if !canonical_file.starts_with(&canonical_root) {
        return Err(WorkflowError::Message(format!(
            "file `{}` is outside project module root `{}`.\n\
             When phoenix.toml is present, check only files under module_src.",
            canonical_file.display(),
            canonical_root.display()
        )));
    }
    Ok(())
}

fn standalone_options(
    entry: &Path,
    args: &FileCommandArgs,
) -> Result<StandaloneOptions, WorkflowError> {
    let module_root = args
        .module_src
        .clone()
        .unwrap_or_else(|| entry.parent().unwrap_or(Path::new(".")).to_path_buf());
    Ok(StandaloneOptions {
        entry: entry.to_path_buf(),
        module_root,
        package_name: args.package_name.clone(),
        path_deps: args.deps.clone(),
    })
}

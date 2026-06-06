//! Project vs standalone workflow resolution.

use std::path::Path;

use phx_compiler::{
    ProjectConfig, ProjectError, StandaloneOptions, discover_project, resolve_project,
};

use crate::args::FileCommandArgs;

/// How `check` / `run` should invoke the compiler.
#[derive(Debug)]
pub enum CompileMode {
    /// Full project workflow (`phoenix.toml` discovered).
    Project {
        /// Loaded project configuration.
        config: ProjectConfig,
    },
    /// Single entry file without a project manifest.
    Standalone {
        /// Standalone compile options.
        options: StandaloneOptions,
    },
}

/// Workflow resolution failure.
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
/// # Errors
///
/// Returns [`WorkflowError`] when project rules are violated.
pub fn resolve_check_mode(
    file: &Path,
    file_args: &FileCommandArgs,
    project_root: Option<&Path>,
) -> Result<CompileMode, WorkflowError> {
    if let Ok(config) = try_discover_project(file, project_root) {
        validate_file_in_project(file, &config)?;
        return Ok(CompileMode::Project { config });
    }
    let options = standalone_options(file, file_args)?;
    Ok(CompileMode::Standalone { options })
}

/// Resolves compile mode for `phx run`.
///
/// # Errors
///
/// Returns [`WorkflowError`] when project rules are violated or a file is required.
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
        return Ok(CompileMode::Project { config });
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
/// # Errors
///
/// Returns [`WorkflowError`] when no project is found.
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

//! Standalone (non-project) compile and check entry points.

use std::path::PathBuf;

use phx_bytecode::BytecodeModule;
use phx_diagnostics::DiagnosticBag;

use crate::compile::{CompileError, DiagnosticContext};
use crate::lower::lower;
use crate::modules::{ProgramLoadContext, load_program_with_context, resolve_loaded_program};
use crate::project::ProjectError;
use crate::typeck::type_check;

/// Options for compiling a single entry file without `phoenix.toml`.
#[derive(Debug, Clone)]
pub struct StandaloneOptions {
    /// Entry `.phx` file to compile or check.
    pub entry: PathBuf,
    /// Module root for `#import` resolution (`::` paths under this directory).
    pub module_root: PathBuf,
    /// Workspace package name override (defaults to module root directory name).
    pub package_name: Option<String>,
    /// Path dependencies: `(package_name, filesystem_path)`.
    pub path_deps: Vec<(String, PathBuf)>,
}

impl StandaloneOptions {
    /// Builds a [`ProgramLoadContext`] for this standalone invocation.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when a path dependency is invalid.
    pub fn load_context(&self) -> Result<ProgramLoadContext, ProjectError> {
        ProgramLoadContext::from_standalone(
            &self.module_root,
            self.package_name.clone(),
            &self.path_deps,
        )
    }
}

/// Type-checks a standalone entry file using a pre-built load context.
///
/// # Errors
///
/// Returns [`CompileError`] on parse, resolve, type-check, or I/O failure.
pub fn check_standalone_with_context(
    opts: &StandaloneOptions,
    ctx: &ProgramLoadContext,
) -> Result<(), CompileError> {
    let _ = check_standalone_unit_with_context(opts, ctx)?;
    Ok(())
}

/// Type-checks a standalone entry and returns the compilation unit.
///
/// # Errors
///
/// Returns [`CompileError`] on failure.
pub fn check_standalone_unit_with_context(
    opts: &StandaloneOptions,
    ctx: &ProgramLoadContext,
) -> Result<crate::unit::CompilationUnit, CompileError> {
    let source = std::fs::read_to_string(&opts.entry).map_err(CompileError::Io)?;
    let mut bag = DiagnosticBag::new();
    let Some(loaded) = load_program_with_context(&opts.entry, ctx, None, &mut bag) else {
        return Err(CompileError::Resolve {
            bag,
            context: None,
            prior_parse: None,
        });
    };
    let diag_ctx = DiagnosticContext::from_loaded(&loaded.modules, loaded.interner.clone());
    let resolved = resolve_loaded_program(loaded).map_err(|bag| CompileError::Resolve {
        bag,
        context: Some(diag_ctx.clone()),
        prior_parse: None,
    })?;
    let typeck_ctx = DiagnosticContext::from_resolved(&resolved);
    let typed = type_check(resolved).map_err(|bag| CompileError::TypeCheck {
        bag,
        context: typeck_ctx,
        prior_parse: None,
    })?;
    Ok(crate::unit::CompilationUnit {
        path: Some(opts.entry.clone()),
        source,
        typed,
    })
}

/// Compiles a standalone entry to bytecode.
///
/// # Errors
///
/// Returns [`CompileError`] on failure.
pub fn compile_standalone_with_context(
    opts: &StandaloneOptions,
    ctx: &ProgramLoadContext,
) -> Result<BytecodeModule, CompileError> {
    let unit = check_standalone_unit_with_context(opts, ctx)?;
    let ctx_diag = DiagnosticContext::from_resolved(&unit.typed.resolved);
    let ir = lower(&unit.typed).map_err(|bag| CompileError::Lower {
        bag,
        context: ctx_diag,
    })?;
    crate::codegen::codegen(&ir, &unit.typed).map_err(CompileError::Codegen)
}

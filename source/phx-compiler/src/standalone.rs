//! Compile and type-check a single entry file without `phoenix.toml`.
//!
//! Tier-2 driver API for the CLI and in-repo tooling when no project manifest is present.
//! [`StandaloneOptions`] describes the entry path, module root for `#import` resolution, optional
//! package name override, and path dependencies. Build a [`ProgramLoadContext`] via
//! [`StandaloneOptions::load_context`], then call one of the `*_with_context` entry points.
//!
//! For project-based workflows, use [`crate::build_project`] or [`crate::check_project_file`]
//! instead. For stable embedder APIs, prefer [`crate::facade`].
//!
//! ## Pipeline
//!
//! All entry points share the same front half:
//!
//! ```text
//! entry .phx → read → load_program_with_context → resolve → type_check → CompilationUnit
//! ```
//!
//! [`compile_standalone_with_context`] continues through lower and codegen via
//! [`crate::compile_compilation_unit`].
//!
//! ## Entry points
//!
//! | Function | Stops after | Returns |
//! |----------|-------------|---------|
//! | [`check_standalone_with_context`] | type-check | `()` |
//! | [`check_standalone_unit_with_context`] | type-check | [`CompilationUnit`](crate::unit::CompilationUnit) |
//! | [`compile_standalone_with_context`] | codegen | [`BytecodeModule`] |
//!
//! Reuse a single [`ProgramLoadContext`] across calls when checking then compiling the same
//! standalone tree to avoid re-resolving path dependencies.

use std::path::PathBuf;

use phx_bytecode::BytecodeModule;
use phx_diagnostics::DiagnosticBag;

use crate::compile::{CompileError, DiagnosticContext, compile_compilation_unit};
use crate::modules::{ProgramLoadContext, load_program_with_context, resolve_loaded_program};
use crate::project::ProjectError;
use crate::typeck::type_check;

/// Configuration for compiling or checking one entry file outside a Phoenix project.
///
/// Built by the CLI for `phx check path/to/main.phx` and similar invocations where no
/// `phoenix.toml` is present. Path dependencies are resolved relative to [`Self::module_root`].
#[derive(Debug, Clone)]
pub struct StandaloneOptions {
    /// Absolute or relative path to the entry `.phx` file.
    ///
    /// Becomes the compilation unit path and the starting module for
    /// [`load_program_with_context`].
    pub entry: PathBuf,
    /// Directory used as the module root for `#import` resolution (`::` paths resolve here).
    ///
    /// Typically the directory containing the entry file or a parent `src/` tree.
    pub module_root: PathBuf,
    /// Workspace package name override; defaults to the final component of [`Self::module_root`].
    ///
    /// Controls the root package namespace for absolute imports when multiple standalone
    /// trees are linked via path dependencies.
    pub package_name: Option<String>,
    /// Path dependencies as `(package_name, filesystem_path)` pairs.
    ///
    /// Each path is resolved relative to [`Self::module_root`]; names must match the depended
    /// package's declared name when that dependency is itself a project.
    pub path_deps: Vec<(String, PathBuf)>,
}

impl StandaloneOptions {
    /// Builds a [`ProgramLoadContext`] for this standalone invocation.
    ///
    /// Resolves path dependencies relative to [`Self::module_root`] and applies the optional
    /// [`Self::package_name`] override. The returned context is immutable for the duration of
    /// check/compile calls and may be reused across [`check_standalone_with_context`] and
    /// [`compile_standalone_with_context`] on the same options.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when a path dependency is missing, invalid, or duplicates a
    /// package name.
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
/// Runs parse, resolve, and type-check only — no lowering or codegen. Discards the
/// [`crate::unit::CompilationUnit`]; use [`check_standalone_unit_with_context`] when the typed
/// program is needed (e.g. lint or `.pxi` export).
///
/// # Errors
///
/// Returns [`CompileError::Io`] when the entry file cannot be read.
/// Returns [`CompileError::Resolve`] when module loading or name resolution fails.
/// Returns [`CompileError::TypeCheck`] when type checking fails.
pub fn check_standalone_with_context(
    opts: &StandaloneOptions,
    ctx: &ProgramLoadContext,
) -> Result<(), CompileError> {
    let _ = check_standalone_unit_with_context(opts, ctx)?;
    Ok(())
}

/// Type-checks a standalone entry and returns the compilation unit.
///
/// On success, the returned [`crate::unit::CompilationUnit`] holds the entry source text and
/// [`crate::unstable::TypedProgram`]. Pass it to [`crate::compile_compilation_unit`] for bytecode
/// emission, or to lint / interface export helpers.
///
/// # Errors
///
/// Returns [`CompileError::Io`] when the entry file cannot be read.
/// Returns [`CompileError::Resolve`] when module loading or name resolution fails.
/// Returns [`CompileError::TypeCheck`] when type checking fails.
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
/// Runs the full pipeline: parse, resolve, type-check, lower, and codegen. The returned
/// [`BytecodeModule`] should still be verified with [`phx_bytecode::verify`] before execution.
///
/// # Errors
///
/// Returns [`CompileError::Io`] when the entry file cannot be read.
/// Returns [`CompileError::Resolve`] when module loading or name resolution fails.
/// Returns [`CompileError::TypeCheck`] when type checking fails.
/// Returns [`CompileError::Lower`], [`CompileError::IrValidate`], or [`CompileError::Codegen`]
/// when back-end passes fail after a successful type-check.
pub fn compile_standalone_with_context(
    opts: &StandaloneOptions,
    ctx: &ProgramLoadContext,
) -> Result<BytecodeModule, CompileError> {
    let unit = check_standalone_unit_with_context(opts, ctx)?;
    compile_compilation_unit(&unit)
}

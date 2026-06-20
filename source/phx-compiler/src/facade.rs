//! Stable entry points for external tools (LSP, SDK, embedders).
//!
//! Wraps the [`compile`](crate::compile) module without duplicating pipeline logic. Returns
//! opaque success types ([`CheckOutput`], [`CompileOutput`]) so callers are not coupled to
//! internal side tables ([`crate::unstable::TypedProgram`], [`crate::unstable::ResolvedProgram`],
//! [`crate::unstable::CompilationUnit`]).
//!
//! ## When to use the facade
//!
//! | Goal | API | Success type |
//! | --- | --- | --- |
//! | Type-check only (parse → resolve → typeck) | [`check_file`], [`check_file_with_module_path`] | [`CheckOutput`] |
//! | Full compile through PHX0 emission | [`compile_to_module`], [`compile_to_module_with_module_path`] | [`CompileOutput`] |
//!
//! - **Type-check only** — stop after type-check; format failures with
//!   [`CompileError::format_with_modules`](crate::CompileError::format_with_modules).
//! - **Bytecode emission** — run [`phx_bytecode::verify`] on [`CompileOutput::bytecode`] before
//!   loading or executing in `phx-vm`.
//! - **Project builds with linking** — use [`crate::build_project`] instead; the facade compiles
//!   a single executable module entry, not a full `phoenix.toml` workspace link step.
//!
//! For in-repo drivers that need the typed program (lint, `.pxi` export, incremental build), use
//! [`crate::check_file`] and [`crate::compile_compilation_unit`] via [`crate::unstable`] instead.
//!
//! ## Facade vs driver APIs
//!
//! Root-level [`crate::check_file`] and [`crate::compile_to_module`] run the same pipeline but
//! return [`crate::unstable::CompilationUnit`] or raw [`BytecodeModule`](phx_bytecode::BytecodeModule).
//! The facade hides those types so embedders do not depend on compiler graph layout.
//!
//! ## Module and project discovery
//!
//! - [`check_file`] / [`compile_to_module`] — resolve `#import` relative to the entry file's
//!   directory and walk upward for `phoenix.toml` when present (same rules as the `phx` CLI).
//! - [`check_file_with_module_path`] / [`compile_to_module_with_module_path`] — treat
//!   `module_root` as the root for `#import` paths when the entry file is not already under the
//!   intended module tree (e.g. scratch buffers written to a temp fixtures layout).
//!
//! ## Error handling
//!
//! All entry points return [`CompileError`]. Multi-file failures may include a
//! [`DiagnosticContext`](crate::DiagnosticContext) inside the error; pass optional entry source,
//! path, and module slices to [`CompileError::format_with_modules`] for caret-aligned output.

use std::path::Path;

use phx_bytecode::BytecodeModule;

use crate::compile::{
    CompileError, check_file as check_file_inner,
    check_file_with_module_path as check_file_with_module_path_inner,
    compile_to_module as compile_to_module_inner,
    compile_to_module_with_module_path as compile_to_module_with_module_path_inner,
};

/// Successful end-to-end compile of one executable module.
///
/// Produced by [`compile_to_module`] and [`compile_to_module_with_module_path`]. The bytecode is
/// ready for verification and loading; this crate does not run the VM.
///
/// After success, callers should run [`phx_bytecode::verify`] on [`bytecode`](Self::bytecode)
/// before handing the image to `phx-vm` or serializing to disk.
#[derive(Debug)]
pub struct CompileOutput {
    /// PHX0 module image (caller should still run [`phx_bytecode::verify`] before load/run).
    pub bytecode: BytecodeModule,
}

/// Successful type-check of one module (no codegen).
///
/// Produced by [`check_file`] and [`check_file_with_module_path`]. Carries no payload — success
/// means parse, resolve, and type-check completed with no errors. Lint warnings may still be
/// emitted by the driver but do not fail these APIs.
///
/// Use [`CheckOutput`] as proof of validity when an embedder only needs diagnostics on failure
/// and does not need [`crate::unstable::TypedProgram`] for further analysis.
#[derive(Debug)]
pub struct CheckOutput;

/// Type-checks `path` (multi-file when `#import` is used).
///
/// Reads the entry file at `path`, loads imported modules from disk, and runs resolve + type-check.
/// Does not lower or emit bytecode.
///
/// # Errors
///
/// Returns [`CompileError::Parse`] on lex/parse failure, [`CompileError::Resolve`] when imports
/// or name binding fail, and [`CompileError::TypeCheck`] when ownership or type rules are
/// violated. Format with [`CompileError::format_with_modules`] for multi-file carets.
///
/// # Panics
///
/// Never panics on malformed user source or missing files — I/O and diagnostic failures surface
/// as [`CompileError`].
///
/// # Examples
///
/// ```no_run
/// use phx_compiler::{CompileError, facade};
/// use std::path::Path;
///
/// fn check_main(entry: &Path) -> Result<(), String> {
///     facade::check_file(entry).map_err(|e| {
///         e.format_with_modules(None, entry.to_str(), None, None)
///     })?;
///     Ok(())
/// }
/// ```
pub fn check_file(path: &Path) -> Result<CheckOutput, CompileError> {
    check_file_inner(path).map(|_| CheckOutput)
}

/// Type-checks `path` with an explicit module root for `#import` resolution.
///
/// Same pipeline as [`check_file`], but `#import` paths are resolved relative to `module_root`
/// instead of inferring the root from `path` alone. Use when the entry file lives outside the
/// module tree (for example, a temp copy of a fixture entry under `tests/cli/fixtures/...`).
///
/// # Errors
///
/// Same as [`check_file`].
///
/// # Panics
///
/// Never panics on malformed user source or missing files — I/O and diagnostic failures surface
/// as [`CompileError`].
pub fn check_file_with_module_path(
    path: &Path,
    module_root: &Path,
) -> Result<CheckOutput, CompileError> {
    check_file_with_module_path_inner(path, module_root).map(|_| CheckOutput)
}

/// Reads `path`, type-checks, lowers, and emits bytecode ready for verify/run.
///
/// Runs the full pipeline: parse → resolve → type-check → lower → codegen. On success, returns
/// a [`CompileOutput`] whose [`CompileOutput::bytecode`] should be verified before execution.
///
/// # Errors
///
/// Same variants as [`check_file`], plus [`CompileError::Lower`] and [`CompileError::Codegen`]
/// when lowering or bytecode emission fails.
///
/// # Panics
///
/// Never panics on malformed user source or missing files — I/O and diagnostic failures surface
/// as [`CompileError`].
///
/// # Examples
///
/// ```no_run
/// use phx_compiler::facade;
/// use std::path::Path;
///
/// fn compile(entry: &Path) -> Result<(), Box<dyn std::error::Error>> {
///     let out = facade::compile_to_module(entry)?;
///     phx_bytecode::verify(&out.bytecode)?;
///     Ok(())
/// }
/// ```
pub fn compile_to_module(path: &Path) -> Result<CompileOutput, CompileError> {
    compile_to_module_inner(path).map(|bytecode| CompileOutput { bytecode })
}

/// Same as [`compile_to_module`] with an explicit module root.
///
/// Combines the module-root semantics of [`check_file_with_module_path`] with full codegen from
/// [`compile_to_module`].
///
/// # Errors
///
/// Same as [`compile_to_module`] and [`check_file_with_module_path`].
///
/// # Panics
///
/// Never panics on malformed user source or missing files — I/O and diagnostic failures surface
/// as [`CompileError`].
pub fn compile_to_module_with_module_path(
    path: &Path,
    module_root: &Path,
) -> Result<CompileOutput, CompileError> {
    compile_to_module_with_module_path_inner(path, module_root)
        .map(|bytecode| CompileOutput { bytecode })
}

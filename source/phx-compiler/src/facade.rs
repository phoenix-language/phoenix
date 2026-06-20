//! Stable entry points for external tools (LSP, SDK, embedders).
//!
//! Wraps the [`compile`](crate::compile) module without duplicating pipeline logic. Returns
//! opaque success types ([`CheckOutput`], [`CompileOutput`]) so callers are not coupled to
//! internal side tables ([`crate::unstable::TypedProgram`], [`crate::unstable::ResolvedProgram`],
//! [`crate::unstable::CompilationUnit`]).
//!
//! ## When to use the facade
//!
//! - **Type-check only** — [`check_file`] or [`check_file_with_module_path`]; diagnostics via
//!   [`CompileError::format_with_modules`](crate::CompileError::format_with_modules).
//! - **Bytecode emission** — [`compile_to_module`] or [`compile_to_module_with_module_path`];
//!   run [`phx_bytecode::verify`] on [`CompileOutput::bytecode`] before execution.
//!
//! For in-repo drivers that need the typed program (lint, `.pxi` export, incremental build), use
//! [`crate::check_file`] and [`crate::compile_compilation_unit`] instead.

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
#[derive(Debug)]
pub struct CompileOutput {
    /// PHX0 module image (caller should still run [`phx_bytecode::verify`] before load/run).
    pub bytecode: BytecodeModule,
}

/// Successful type-check of one module (no codegen).
///
/// Produced by [`check_file`] and [`check_file_with_module_path`]. Carries no payload — success
/// means parse, resolve, and type-check completed with no errors.
#[derive(Debug)]
pub struct CheckOutput;

/// Type-checks `path` (multi-file when `#import` is used).
///
/// # Errors
///
/// Returns [`CompileError`] on parse, resolve, or type-check failure.
pub fn check_file(path: &Path) -> Result<CheckOutput, CompileError> {
    check_file_inner(path).map(|_| CheckOutput)
}

/// Type-checks `path` with an explicit module root for `#import` resolution.
///
/// # Errors
///
/// Same as [`check_file`].
pub fn check_file_with_module_path(
    path: &Path,
    module_root: &Path,
) -> Result<CheckOutput, CompileError> {
    check_file_with_module_path_inner(path, module_root).map(|_| CheckOutput)
}

/// Reads `path`, type-checks, lowers, and emits bytecode ready for verify/run.
///
/// # Errors
///
/// Same as [`check_file`].
pub fn compile_to_module(path: &Path) -> Result<CompileOutput, CompileError> {
    compile_to_module_inner(path).map(|bytecode| CompileOutput { bytecode })
}

/// Same as [`compile_to_module`] with an explicit module root.
///
/// # Errors
///
/// Same as [`check_file_with_module_path`].
pub fn compile_to_module_with_module_path(
    path: &Path,
    module_root: &Path,
) -> Result<CompileOutput, CompileError> {
    compile_to_module_with_module_path_inner(path, module_root)
        .map(|bytecode| CompileOutput { bytecode })
}

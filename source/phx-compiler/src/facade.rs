//! Stable entry points for external tools (LSP, SDK, embedders).
//!
//! Wraps [`crate::compile`] without duplicating pipeline logic. Internal compiler graphs
//! ([`crate::TypedProgram`], [`crate::ResolvedProgram`], [`crate::CompilationUnit`]) remain
//! accessible in-tree but are not part of this facade.

use std::path::Path;

use phx_bytecode::BytecodeModule;

use crate::compile::{
    CompileError, check_file as check_file_inner,
    check_file_with_module_path as check_file_with_module_path_inner,
    compile_to_module as compile_to_module_inner,
    compile_to_module_with_module_path as compile_to_module_with_module_path_inner,
};

/// Successful end-to-end compile of one executable module.
#[derive(Debug)]
pub struct CompileOutput {
    /// Verified-ready bytecode (caller should still run [`phx_bytecode::verify`]).
    pub bytecode: BytecodeModule,
}

/// Successful type-check of one module (no codegen).
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

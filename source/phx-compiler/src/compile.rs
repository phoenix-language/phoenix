//! Compile driver: parse, resolve, type-check, lower, and codegen.
//!
//! [`compile_source`] keeps an owned [`String`](crate::unit::CompilationUnit::source) so the
//! resulting [`CompilationUnit`] is independent of the caller's buffer.

use std::io;
use std::path::Path;

use phx_bytecode::BytecodeModule;
use phx_diagnostics::{DiagnosticBag, ParseError, TypeCheckBag};
use phx_syntax::parse;

use crate::codegen::codegen;
use crate::lower::lower;
use crate::resolver::resolve;
use crate::typeck::type_check;
use crate::unit::CompilationUnit;

/// Failure during `compile_source` or `check_file`.
#[derive(Debug)]
pub enum CompileError {
    /// Lexical or parse failure.
    Parse(ParseError),
    /// One or more resolve errors.
    Resolve(DiagnosticBag),
    /// One or more type-check errors.
    TypeCheck(TypeCheckBag),
    /// Failed to read source from disk.
    Io(io::Error),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{e}"),
            Self::Resolve(bag) => write!(f, "{bag}"),
            Self::TypeCheck(bag) => write!(f, "{bag}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for CompileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(e) => Some(e),
            Self::Resolve(bag) => Some(bag),
            Self::TypeCheck(bag) => Some(bag),
            Self::Io(e) => Some(e),
        }
    }
}

/// Parses and resolves Phoenix `source`.
///
/// # Errors
///
/// Returns [`CompileError::Parse`] or [`CompileError::Resolve`] on failure.
pub fn compile_source(source: &str, path: Option<&Path>) -> Result<CompilationUnit, CompileError> {
    let source_file = parse(source).map_err(CompileError::Parse)?;
    let resolved = resolve(&source_file).map_err(CompileError::Resolve)?;
    let typed = type_check(&resolved).map_err(CompileError::TypeCheck)?;
    Ok(CompilationUnit {
        path: path.map(Path::to_path_buf),
        source: source.to_owned(),
        typed,
    })
}

/// Reads `path` and runs [`compile_source`].
///
/// # Errors
///
/// Returns I/O errors, [`CompileError::Parse`], [`CompileError::Resolve`], or [`CompileError::TypeCheck`].
pub fn check_file(path: &Path) -> Result<CompilationUnit, CompileError> {
    let source = std::fs::read_to_string(path).map_err(CompileError::Io)?;
    compile_source(&source, Some(path))
}

/// Reads `path`, type-checks, lowers, and emits bytecode ready for verify/run.
///
/// # Errors
///
/// Same as [`check_file`].
pub fn compile_to_module(path: &Path) -> Result<BytecodeModule, CompileError> {
    let unit = check_file(path)?;
    Ok(codegen(&lower(&unit.typed), &unit.typed.layout))
}

//! Compile driver: parse then resolve.
//!
//! [`compile_source`] keeps an owned [`String`](crate::unit::CompilationUnit::source) so the
//! resulting [`CompilationUnit`] is independent of the caller's buffer.

use std::io;
use std::path::Path;

use phx_diagnostics::{DiagnosticBag, ParseError};
use phx_syntax::parse;

use crate::resolver::resolve;
use crate::unit::CompilationUnit;

/// Failure during `compile_source` or `check_file`.
#[derive(Debug)]
pub enum CompileError {
    /// Lexical or parse failure.
    Parse(ParseError),
    /// One or more resolve errors.
    Resolve(DiagnosticBag),
    /// Failed to read source from disk.
    Io(io::Error),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{e}"),
            Self::Resolve(bag) => write!(f, "{bag}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for CompileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(e) => Some(e),
            Self::Resolve(bag) => Some(bag),
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
    Ok(CompilationUnit {
        path: path.map(Path::to_path_buf),
        source: source.to_owned(),
        resolved,
    })
}

/// Reads `path` and runs [`compile_source`].
///
/// # Errors
///
/// Returns I/O errors, [`CompileError::Parse`], or [`CompileError::Resolve`].
pub fn check_file(path: &Path) -> Result<CompilationUnit, CompileError> {
    let source = std::fs::read_to_string(path).map_err(CompileError::Io)?;
    compile_source(&source, Some(path))
}

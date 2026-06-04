//! Compile driver: parse, resolve, type-check, lower, and codegen.
//!
//! [`compile_source`] keeps an owned [`String`](crate::unit::CompilationUnit::source) so the
//! resulting [`CompilationUnit`] is independent of the caller's buffer.

use std::io;
use std::path::Path;

use crate::resolver::SourceModule;
use phx_bytecode::BytecodeModule;
use phx_diagnostics::{
    DiagnosticBag, ParseError, TypeCheckBag, format_span_message, format_typecheck_error,
};
use phx_syntax::parse;

use crate::codegen::codegen;
use crate::lower::lower;
use crate::modules::{load_crate, resolve_crate};
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

impl CompileError {
    /// Returns a user-facing message, optionally with source carets when `source` is provided.
    #[must_use]
    pub fn format_with_source(&self, source: Option<&str>) -> String {
        self.format_with_modules(source, None)
    }

    /// Formats this error using the entry `source` and optional multi-module sources.
    #[must_use]
    pub fn format_with_modules(
        &self,
        entry_source: Option<&str>,
        modules: Option<&[SourceModule]>,
    ) -> String {
        match self {
            Self::Parse(e) => format_parse_error(e, entry_source),
            Self::Resolve(bag) => format_resolve_bag(bag, entry_source, modules),
            Self::TypeCheck(bag) => format_typecheck_bag(bag, entry_source, modules),
            Self::Io(e) => format!("I/O error: {e}"),
        }
    }
}

fn format_parse_error(e: &ParseError, source: Option<&str>) -> String {
    if let Some(src) = source
        && let Some(span) = e.span()
    {
        return format_span_message(src, span, &e.to_string());
    }
    e.to_string()
}

fn format_resolve_bag(
    bag: &DiagnosticBag,
    entry_source: Option<&str>,
    modules: Option<&[SourceModule]>,
) -> String {
    if let Some(err) = bag.errors().first()
        && let Some(span) = err.span()
        && let Some((label, src)) = source_for_span(entry_source, modules, span)
    {
        return format_span_message(src, span, &format!("{label}: {err}"));
    }
    bag.to_string()
}

fn format_typecheck_bag(
    bag: &TypeCheckBag,
    entry_source: Option<&str>,
    modules: Option<&[SourceModule]>,
) -> String {
    if let Some(err) = bag.errors().first() {
        if let Some(span) = err.span()
            && let Some((label, src)) = source_for_span(entry_source, modules, span)
        {
            let body = format_typecheck_error(src, err);
            return format!("{label}:\n{body}");
        }
        if let Some(src) = entry_source {
            return format_typecheck_error(src, err);
        }
    }
    bag.to_string()
}

fn source_for_span<'a>(
    entry_source: Option<&'a str>,
    modules: Option<&'a [SourceModule]>,
    span: phx_diagnostics::Span,
) -> Option<(String, &'a str)> {
    if let Some(mods) = modules {
        for m in mods {
            if usize::try_from(span.end).unwrap_or(0) <= m.source.len() {
                return Some((m.filesystem.display().to_string(), m.source.as_str()));
            }
        }
    }
    entry_source.map(|s| ("<entry>".to_owned(), s))
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

/// Reads `path` and runs the full front-end (multi-file when `#import` is used).
///
/// # Errors
///
/// Returns I/O errors, [`CompileError::Parse`], [`CompileError::Resolve`], or [`CompileError::TypeCheck`].
pub fn check_file(path: &Path) -> Result<CompilationUnit, CompileError> {
    check_file_with_module_path(path, path.parent().unwrap_or(Path::new(".")))
}

/// Reads `path` using `module_root` for `#import` resolution.
///
/// # Errors
///
/// Same as [`check_file`].
pub fn check_file_with_module_path(
    path: &Path,
    module_root: &Path,
) -> Result<CompilationUnit, CompileError> {
    let source = std::fs::read_to_string(path).map_err(CompileError::Io)?;
    let mut bag = DiagnosticBag::new();
    let loaded = load_crate(path, module_root, &mut bag).ok_or(CompileError::Resolve(bag))?;
    let resolved = resolve_crate(loaded).map_err(CompileError::Resolve)?;
    let typed = type_check(&resolved).map_err(CompileError::TypeCheck)?;
    Ok(CompilationUnit {
        path: Some(path.to_path_buf()),
        source,
        typed,
    })
}

/// Reads `path`, type-checks, lowers, and emits bytecode ready for verify/run.
///
/// # Errors
///
/// Same as [`check_file`].
pub fn compile_to_module(path: &Path) -> Result<BytecodeModule, CompileError> {
    compile_to_module_with_module_path(path, path.parent().unwrap_or(Path::new(".")))
}

/// Same as [`compile_to_module`] with an explicit `--module-path` root.
///
/// # Errors
///
/// Same as [`check_file_with_module_path`].
pub fn compile_to_module_with_module_path(
    path: &Path,
    module_root: &Path,
) -> Result<BytecodeModule, CompileError> {
    let unit = check_file_with_module_path(path, module_root)?;
    Ok(codegen(&lower(&unit.typed), &unit.typed))
}

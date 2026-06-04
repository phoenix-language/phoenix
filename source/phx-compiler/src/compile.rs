//! Compile driver: parse, resolve, type-check, lower, and codegen.
//!
//! [`compile_source`] keeps an owned [`String`](crate::unit::CompilationUnit::source) so the
//! resulting [`CompilationUnit`] is independent of the caller's buffer.

use std::io;
use std::path::Path;

use crate::resolver::SourceModule;
use phx_bytecode::BytecodeModule;
use phx_diagnostics::{
    DiagnosticBag, LowerBag, ParseBag, ParseError, TypeCheckBag, format_lex_error,
    format_lower_error, format_resolve_error, format_typecheck_error,
};
use phx_syntax::{Interner, parse};

use crate::codegen::codegen;
use crate::lower::lower;
use crate::modules::{LoadedModule, load_crate, resolve_crate};
use crate::resolver::{ResolvedProgram, resolve};
use crate::typeck::type_check;
use crate::unit::CompilationUnit;

/// Sources and interner needed to format multi-module diagnostics.
///
/// Populated after crate load or resolve so [`CompileError::format_with_modules`] can print
/// carets in the correct file.
#[derive(Debug, Clone)]
pub struct DiagnosticContext {
    /// All modules in the crate (for span → source buffer routing).
    pub modules: Vec<SourceModule>,
    /// Shared interner for symbol names in messages.
    pub interner: Interner,
}

impl DiagnosticContext {
    /// Builds context from a resolved crate.
    #[must_use]
    pub fn from_resolved(resolved: &ResolvedProgram) -> Self {
        Self {
            modules: resolved.modules.clone(),
            interner: resolved.interner.clone(),
        }
    }

    /// Builds context from loaded modules before resolve completes.
    #[must_use]
    pub fn from_loaded(modules: &[LoadedModule], interner: Interner) -> Self {
        Self {
            modules: modules
                .iter()
                .map(|m| SourceModule {
                    id: m.id.index(),
                    logical_path: m.logical_path.display(),
                    filesystem: m.filesystem.clone(),
                    source: m.source.clone(),
                    program: m.program.clone(),
                })
                .collect(),
            interner,
        }
    }
}

/// Failure during `compile_source` or `check_file`.
#[derive(Debug)]
pub enum CompileError {
    /// Lexical or parse failures.
    Parse(ParseBag),
    /// One or more resolve errors.
    Resolve {
        /// Collected errors.
        bag: DiagnosticBag,
        /// Module sources when available (multi-file crate load / resolve).
        context: Option<DiagnosticContext>,
    },
    /// One or more type-check errors.
    TypeCheck {
        /// Collected errors.
        bag: TypeCheckBag,
        /// Module sources and interner from the resolved program.
        context: DiagnosticContext,
    },
    /// One or more IR lowering errors (internal invariant violations).
    Lower {
        /// Collected errors.
        bag: LowerBag,
        /// Module sources and interner from the typed program.
        context: DiagnosticContext,
    },
    /// IR → bytecode codegen failure.
    Codegen(crate::codegen::CodegenError),
    /// Failed to read source from disk.
    Io(io::Error),
}

impl CompileError {
    /// Returns a user-facing message, optionally with source carets when `source` is provided.
    #[must_use]
    pub fn format_with_source(&self, source: Option<&str>) -> String {
        self.format_with_modules(source, None, None)
    }

    /// Formats this error using optional entry `source`, multi-module sources, and interner.
    #[must_use]
    pub fn format_with_modules(
        &self,
        entry_source: Option<&str>,
        modules: Option<&[SourceModule]>,
        interner: Option<&Interner>,
    ) -> String {
        match self {
            Self::Parse(bag) => format_parse_bag(bag, entry_source),
            Self::Resolve { bag, context } => {
                let mods = context.as_ref().map(|c| c.modules.as_slice()).or(modules);
                let intern = context.as_ref().map(|c| &c.interner).or(interner);
                format_resolve_bag(bag, entry_source, mods, intern)
            }
            Self::TypeCheck { bag, context } => format_typecheck_bag(
                bag,
                entry_source,
                Some(&context.modules),
                Some(&context.interner),
            ),
            Self::Lower { bag, context } => {
                format_lower_bag(bag, entry_source, Some(&context.modules))
            }
            Self::Codegen(e) => e.to_string(),
            Self::Io(e) => format!("I/O error: {e}"),
        }
    }

    /// Borrows the resolve error bag when this is a resolve failure.
    #[must_use]
    pub fn resolve_bag(&self) -> Option<&DiagnosticBag> {
        match self {
            Self::Resolve { bag, .. } => Some(bag),
            _ => None,
        }
    }
}

fn format_parse_bag(bag: &ParseBag, source: Option<&str>) -> String {
    let mut parts = Vec::new();
    for err in bag.errors() {
        let msg = match (source, err) {
            (Some(src), ParseError::Lex(e)) => format_lex_error(src, e),
            (Some(src), other) if let Some(span) = other.span() => {
                let code = other.code();
                format!(
                    "{} [{}]",
                    format_span_message_simple(src, span, &other.to_string()),
                    code
                )
            }
            (_, other) => other.to_string(),
        };
        parts.push(msg);
    }
    parts.join("\n---\n")
}

fn format_span_message_simple(source: &str, span: phx_diagnostics::Span, message: &str) -> String {
    phx_diagnostics::format_span_message(source, span, message)
}

fn format_resolve_bag(
    bag: &DiagnosticBag,
    entry_source: Option<&str>,
    modules: Option<&[SourceModule]>,
    interner: Option<&Interner>,
) -> String {
    let default_interner;
    let interner = if let Some(i) = interner {
        i
    } else {
        default_interner = Interner::new();
        &default_interner
    };
    let mut parts = Vec::new();
    for located in bag.errors() {
        let body =
            if let Some((label, src)) = source_for_module(entry_source, modules, located.module) {
                if located.error.span().is_some() {
                    format!(
                        "{label}:\n{}",
                        format_resolve_error(src, interner, &located.error)
                    )
                } else {
                    format!(
                        "{label}: {}",
                        resolve_message_plain(interner, &located.error)
                    )
                }
            } else if let Some(src) = entry_source {
                format_resolve_error(src, interner, &located.error)
            } else {
                resolve_message_plain(interner, &located.error)
            };
        parts.push(body);
    }
    if parts.is_empty() {
        return String::new();
    }
    parts.join("\n---\n")
}

fn resolve_message_plain(interner: &Interner, err: &phx_diagnostics::ResolveError) -> String {
    format!(
        "{} [{}]",
        phx_diagnostics::resolve_message(interner, err),
        err.code()
    )
}

fn format_typecheck_bag(
    bag: &TypeCheckBag,
    entry_source: Option<&str>,
    modules: Option<&[SourceModule]>,
    interner: Option<&Interner>,
) -> String {
    let default_interner;
    let interner = if let Some(i) = interner {
        i
    } else {
        default_interner = Interner::new();
        &default_interner
    };
    let mut parts = Vec::new();
    for located in bag.errors() {
        let body =
            if let Some((label, src)) = source_for_module(entry_source, modules, located.module) {
                format!(
                    "{label}:\n{}",
                    format_typecheck_error(src, interner, &located.error)
                )
            } else if let Some(src) = entry_source {
                format_typecheck_error(src, interner, &located.error)
            } else {
                format!(
                    "{} [{}]",
                    phx_diagnostics::typecheck_message(interner, &located.error),
                    located.error.code()
                )
            };
        parts.push(body);
    }
    if parts.is_empty() {
        return String::new();
    }
    parts.join("\n---\n")
}

fn format_lower_bag(
    bag: &LowerBag,
    entry_source: Option<&str>,
    modules: Option<&[SourceModule]>,
) -> String {
    let mut parts = Vec::new();
    for located in bag.errors() {
        let body =
            if let Some((label, src)) = source_for_module(entry_source, modules, located.module) {
                format!("{label}:\n{}", format_lower_error(src, &located.error))
            } else if let Some(src) = entry_source {
                format_lower_error(src, &located.error)
            } else {
                format!("{} [{}]", located.error, located.error.code())
            };
        parts.push(body);
    }
    if parts.is_empty() {
        return String::new();
    }
    parts.join("\n---\n")
}

fn source_for_module<'a>(
    entry_source: Option<&'a str>,
    modules: Option<&'a [SourceModule]>,
    module_id: u32,
) -> Option<(String, &'a str)> {
    if let Some(mods) = modules
        && let Some(m) = mods.iter().find(|m| m.id == module_id)
    {
        let label = if m.filesystem.as_os_str().is_empty() {
            m.logical_path.clone()
        } else {
            m.filesystem.display().to_string()
        };
        return Some((label, m.source.as_str()));
    }
    if module_id == 0 {
        return entry_source.map(|s| ("<entry>".to_owned(), s));
    }
    None
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(bag) => write!(f, "{bag}"),
            Self::Resolve { bag, .. } => write!(f, "{bag}"),
            Self::TypeCheck { bag, .. } => write!(f, "{bag}"),
            Self::Lower { bag, .. } => write!(f, "{bag}"),
            Self::Codegen(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for CompileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(bag) => Some(bag),
            Self::Resolve { bag, .. } => Some(bag),
            Self::TypeCheck { bag, .. } => Some(bag),
            Self::Lower { bag, .. } => Some(bag),
            Self::Codegen(e) => Some(e),
            Self::Io(e) => Some(e),
        }
    }
}

/// Parses and resolves Phoenix `source` as a **single compilation unit** (no crate loader).
///
/// `#import` is not supported: resolution uses [`resolve`] on one file only, so imports fail with
/// [`crate::resolver::ResolveError::ImportNotSupported`]. For multi-file programs use
/// [`compile_source_with_module_root`], [`check_file_with_module_path`], or [`build_project`].
///
/// # Errors
///
/// Returns [`CompileError::Parse`] or [`CompileError::Resolve`] on failure.
pub fn compile_source(source: &str, path: Option<&Path>) -> Result<CompilationUnit, CompileError> {
    let source_file = parse(source).map_err(CompileError::Parse)?;
    let resolved =
        resolve(&source_file).map_err(|bag| CompileError::Resolve { bag, context: None })?;
    let ctx = DiagnosticContext::from_resolved(&resolved);
    let typed =
        type_check(&resolved).map_err(|bag| CompileError::TypeCheck { bag, context: ctx })?;
    Ok(CompilationUnit {
        path: path.map(Path::to_path_buf),
        source: source.to_owned(),
        typed,
    })
}

/// Parses and type-checks `source` using the module graph rooted at `path` under `module_root`.
///
/// `path` must exist on disk (reachable via `#import` from that entry). The returned
/// [`CompilationUnit::source`] is the `source` argument (typically the entry file text).
///
/// # Errors
///
/// Returns I/O errors from the loader, [`CompileError::Parse`], [`CompileError::Resolve`], or
/// [`CompileError::TypeCheck`].
pub fn compile_source_with_module_root(
    source: &str,
    path: &Path,
    module_root: &Path,
) -> Result<CompilationUnit, CompileError> {
    let mut bag = DiagnosticBag::new();
    let Some(loaded) = load_crate(path, module_root, &mut bag) else {
        return Err(CompileError::Resolve { bag, context: None });
    };
    let ctx = DiagnosticContext::from_loaded(&loaded.modules, loaded.interner.clone());
    let resolved = resolve_crate(loaded).map_err(|bag| CompileError::Resolve {
        bag,
        context: Some(ctx.clone()),
    })?;
    let typed = type_check(&resolved).map_err(|bag| CompileError::TypeCheck {
        bag,
        context: DiagnosticContext::from_resolved(&resolved),
    })?;
    Ok(CompilationUnit {
        path: Some(path.to_path_buf()),
        source: source.to_owned(),
        typed,
    })
}

/// Reads `path` and runs the full front-end (multi-file when `#import` is used).
///
/// Module root defaults to `path.parent()` (or `"."` if missing), matching `phx check` / `phx run`
/// on a single path. Pass an explicit root via [`check_file_with_module_path`] or CLI
/// `--module-src`.
///
/// # Errors
///
/// Returns I/O errors, [`CompileError::Parse`], [`CompileError::Resolve`], or [`CompileError::TypeCheck`].
pub fn check_file(path: &Path) -> Result<CompilationUnit, CompileError> {
    check_file_with_module_path(path, path.parent().unwrap_or(Path::new(".")))
}

/// Reads `path` using `module_root` for `#import` resolution (`::` paths under that directory).
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
    let Some(loaded) = load_crate(path, module_root, &mut bag) else {
        let context = None;
        return Err(CompileError::Resolve { bag, context });
    };
    let ctx = DiagnosticContext::from_loaded(&loaded.modules, loaded.interner.clone());
    let resolved = resolve_crate(loaded).map_err(|bag| CompileError::Resolve {
        bag,
        context: Some(ctx.clone()),
    })?;
    let typed = type_check(&resolved).map_err(|bag| CompileError::TypeCheck {
        bag,
        context: DiagnosticContext::from_resolved(&resolved),
    })?;
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
    let ctx = DiagnosticContext::from_resolved(&unit.typed.resolved);
    let ir = lower(&unit.typed).map_err(|bag| CompileError::Lower { bag, context: ctx })?;
    codegen(&ir, &unit.typed).map_err(CompileError::Codegen)
}

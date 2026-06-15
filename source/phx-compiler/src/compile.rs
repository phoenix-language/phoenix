//! Compile driver: parse, resolve, type-check, lower, and codegen.
//!
//! [`compile_source`] keeps an owned [`String`](crate::unit::CompilationUnit::source) so the
//! resulting [`CompilationUnit`] is independent of the caller's buffer.

use std::io;
use std::path::Path;

use crate::resolver::SourceModule;
use phx_bytecode::BytecodeModule;
use phx_diagnostics::{
    DiagnosticBag, DiagnosticStyle, IrBag, LintBag, LowerBag, ParseBag, PlainStyle, SpanContext,
    TypeCheckBag, format_ir_error_styled, format_lints_styled, format_lower_error_styled,
    format_parse_bag_styled, format_resolve_error_styled, format_typecheck_error_styled,
    join_diagnostics,
};
use phx_syntax::{Interner, parse};

use crate::cfg::{CompileCfg, strip_cfg};
use crate::codegen::codegen;
use crate::derive::expand_derives;
use crate::lint::lint_program;
use crate::lower::lower;
use crate::modules::{
    LoadedModule, ProgramLoadContext, load_program, load_program_with_context,
    resolve_loaded_program,
};
use crate::project::{BuildLayout, ProjectConfig};
use crate::resolver::{ResolvedProgram, resolve};
use crate::typeck::type_check;
use crate::unit::CompilationUnit;

/// Sources and interner needed to format multi-module diagnostics.
///
/// Populated after program load or resolve so [`CompileError::format_with_modules`] can print
/// carets in the correct file.
#[derive(Debug, Clone)]
pub struct DiagnosticContext {
    /// All modules in the loaded program (for span → source buffer routing).
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
        /// Parse errors from an earlier best-effort stage, when present.
        prior_parse: Option<Box<ParseBag>>,
    },
    /// One or more type-check errors.
    TypeCheck {
        /// Collected errors.
        bag: TypeCheckBag,
        /// Module sources and interner from the resolved program.
        context: DiagnosticContext,
        /// Parse errors from an earlier best-effort stage, when present.
        prior_parse: Option<Box<ParseBag>>,
    },
    /// One or more IR lowering errors (internal invariant violations).
    Lower {
        /// Collected errors.
        bag: LowerBag,
        /// Module sources and interner from the typed program.
        context: DiagnosticContext,
    },
    /// One or more IR validation errors (internal invariant violations).
    IrValidate {
        /// Collected errors.
        bag: IrBag,
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
        self.format_with_modules(source, None, None, None)
    }

    /// Formats this error using optional entry `source`, multi-module sources, and interner.
    #[must_use]
    pub fn format_with_modules(
        &self,
        entry_source: Option<&str>,
        entry_path: Option<&str>,
        modules: Option<&[SourceModule]>,
        interner: Option<&Interner>,
    ) -> String {
        self.format_with_modules_styled(entry_source, entry_path, modules, interner, &PlainStyle)
    }

    /// Formats this error with an optional [`DiagnosticStyle`] (colors when using ANSI style).
    #[must_use]
    pub fn format_with_modules_styled(
        &self,
        entry_source: Option<&str>,
        entry_path: Option<&str>,
        modules: Option<&[SourceModule]>,
        interner: Option<&Interner>,
        style: &dyn DiagnosticStyle,
    ) -> String {
        match self {
            Self::Parse(bag) => format_parse_bag_styled(
                bag,
                entry_source,
                SpanContext {
                    file_path: entry_path,
                    logical_module: None,
                },
                style,
            ),
            Self::Resolve {
                bag,
                context,
                prior_parse,
            } => {
                let mods = context.as_ref().map(|c| c.modules.as_slice()).or(modules);
                let intern = context.as_ref().map(|c| &c.interner).or(interner);
                let stage = format_resolve_bag(bag, entry_source, entry_path, mods, intern, style);
                prepend_parse_bag(
                    prior_parse.as_deref(),
                    &stage,
                    entry_source,
                    entry_path,
                    style,
                )
            }
            Self::TypeCheck {
                bag,
                context,
                prior_parse,
            } => {
                let stage = format_typecheck_bag(
                    bag,
                    entry_source,
                    entry_path,
                    Some(&context.modules),
                    Some(&context.interner),
                    style,
                );
                prepend_parse_bag(
                    prior_parse.as_deref(),
                    &stage,
                    entry_source,
                    entry_path,
                    style,
                )
            }
            Self::Lower { bag, context } => {
                format_lower_bag(bag, entry_source, entry_path, Some(&context.modules), style)
            }
            Self::IrValidate { bag, context } => {
                format_ir_bag(bag, entry_source, entry_path, Some(&context.modules), style)
            }
            Self::Codegen(e) => style.plain_error(&e.to_string()),
            Self::Io(e) => style.plain_error(&format!("I/O error: {e}")),
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

struct ModuleSource<'a> {
    file_path: String,
    logical_module: Option<String>,
    source: &'a str,
}

fn prepend_parse_bag(
    prior_parse: Option<&ParseBag>,
    stage: &str,
    entry_source: Option<&str>,
    entry_path: Option<&str>,
    style: &dyn DiagnosticStyle,
) -> String {
    match prior_parse {
        Some(bag) => join_diagnostics(
            style,
            &[
                format_parse_bag_styled(
                    bag,
                    entry_source,
                    SpanContext {
                        file_path: entry_path,
                        logical_module: None,
                    },
                    style,
                ),
                stage.to_owned(),
            ],
        ),
        None => stage.to_owned(),
    }
}

fn format_resolve_bag(
    bag: &DiagnosticBag,
    entry_source: Option<&str>,
    entry_path: Option<&str>,
    modules: Option<&[SourceModule]>,
    interner: Option<&Interner>,
    style: &dyn DiagnosticStyle,
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
        let body = if let Some(ms) =
            source_for_module(entry_source, entry_path, modules, located.module)
        {
            let ctx = SpanContext {
                file_path: Some(&ms.file_path),
                logical_module: ms.logical_module.as_deref(),
            };
            if located.error.span().is_some() {
                format_resolve_error_styled(ms.source, interner, &located.error, style, ctx)
            } else {
                style.error_header(
                    located.error.code(),
                    &phx_diagnostics::resolve_message(interner, &located.error),
                )
            }
        } else if let Some(src) = resolve_error_source(
            entry_source,
            entry_path,
            modules,
            located.module,
            &located.error,
        ) {
            let ctx = resolve_error_context(entry_path);
            format_resolve_error_styled(src, interner, &located.error, style, ctx)
        } else {
            style.error_header(
                located.error.code(),
                &phx_diagnostics::resolve_message(interner, &located.error),
            )
        };
        parts.push(body);
    }
    join_diagnostics(style, &parts)
}

fn format_typecheck_bag(
    bag: &TypeCheckBag,
    entry_source: Option<&str>,
    entry_path: Option<&str>,
    modules: Option<&[SourceModule]>,
    interner: Option<&Interner>,
    style: &dyn DiagnosticStyle,
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
        let body = if let Some(ms) =
            source_for_module(entry_source, entry_path, modules, located.module)
        {
            let ctx = SpanContext {
                file_path: Some(&ms.file_path),
                logical_module: ms.logical_module.as_deref(),
            };
            format_typecheck_error_styled(ms.source, interner, &located.error, style, ctx)
        } else if let Some(src) = entry_source.filter(|_| located.module == 0) {
            let ctx = SpanContext {
                file_path: entry_path,
                logical_module: None,
            };
            format_typecheck_error_styled(src, interner, &located.error, style, ctx)
        } else {
            style.error_header(
                located.error.code(),
                &phx_diagnostics::typecheck_message(interner, &located.error),
            )
        };
        parts.push(body);
    }
    join_diagnostics(style, &parts)
}

fn format_lower_bag(
    bag: &LowerBag,
    entry_source: Option<&str>,
    entry_path: Option<&str>,
    modules: Option<&[SourceModule]>,
    style: &dyn DiagnosticStyle,
) -> String {
    let mut parts = Vec::new();
    for located in bag.errors() {
        let body = if let Some(ms) =
            source_for_module(entry_source, entry_path, modules, located.module)
        {
            let ctx = SpanContext {
                file_path: Some(&ms.file_path),
                logical_module: ms.logical_module.as_deref(),
            };
            format_lower_error_styled(ms.source, &located.error, style, ctx)
        } else if let Some(src) = entry_source.filter(|_| located.module == 0) {
            let ctx = SpanContext {
                file_path: entry_path,
                logical_module: None,
            };
            format_lower_error_styled(src, &located.error, style, ctx)
        } else {
            style.error_header(located.error.code(), &located.error.to_string())
        };
        parts.push(body);
    }
    join_diagnostics(style, &parts)
}

fn format_ir_bag(
    bag: &IrBag,
    entry_source: Option<&str>,
    entry_path: Option<&str>,
    modules: Option<&[SourceModule]>,
    style: &dyn DiagnosticStyle,
) -> String {
    let mut parts = Vec::new();
    for located in bag.errors() {
        let body = if let Some(ms) =
            source_for_module(entry_source, entry_path, modules, located.module)
        {
            let ctx = SpanContext {
                file_path: Some(&ms.file_path),
                logical_module: ms.logical_module.as_deref(),
            };
            format_ir_error_styled(ms.source, &located.error, style, ctx)
        } else if let Some(src) = entry_source.filter(|_| located.module == 0) {
            let ctx = SpanContext {
                file_path: entry_path,
                logical_module: None,
            };
            format_ir_error_styled(src, &located.error, style, ctx)
        } else {
            style.error_header(located.error.code(), &located.error.to_string())
        };
        parts.push(body);
    }
    join_diagnostics(style, &parts)
}

#[cfg(any(debug_assertions, test))]
pub(crate) fn debug_validate_ir(
    ir: &crate::ir::IrModule,
    typed: &crate::typeck::TypedProgram,
    context: DiagnosticContext,
) -> Result<(), CompileError> {
    crate::ir::validate_ir(ir, typed).map_err(|bag| CompileError::IrValidate { bag, context })
}

#[cfg(not(any(debug_assertions, test)))]
fn debug_validate_ir(
    _ir: &crate::ir::IrModule,
    _typed: &crate::typeck::TypedProgram,
    _context: DiagnosticContext,
) -> Result<(), CompileError> {
    Ok(())
}

fn resolve_error_source<'a>(
    entry_source: Option<&'a str>,
    entry_path: Option<&str>,
    _modules: Option<&[SourceModule]>,
    module_id: u32,
    err: &phx_diagnostics::ResolveError,
) -> Option<&'a str> {
    match err {
        phx_diagnostics::ResolveError::ModuleParse { path, .. }
        | phx_diagnostics::ResolveError::ModuleIo { path, .. } => {
            if entry_path == Some(path.as_str()) {
                entry_source
            } else {
                None
            }
        }
        _ => entry_source.filter(|_| module_id == 0),
    }
}

fn resolve_error_context(entry_path: Option<&str>) -> SpanContext<'_> {
    SpanContext {
        file_path: entry_path,
        logical_module: None,
    }
}

fn source_for_module<'a>(
    entry_source: Option<&'a str>,
    entry_path: Option<&str>,
    modules: Option<&'a [SourceModule]>,
    module_id: u32,
) -> Option<ModuleSource<'a>> {
    if let Some(mods) = modules
        && let Some(m) = mods.iter().find(|m| m.id == module_id)
    {
        let file_path = if m.filesystem.as_os_str().is_empty() {
            entry_path.unwrap_or("<entry>").to_owned()
        } else {
            m.filesystem.display().to_string()
        };
        return Some(ModuleSource {
            file_path,
            logical_module: Some(m.logical_path.clone()),
            source: m.source.as_ref(),
        });
    }
    if module_id == 0 {
        return entry_source.map(|s| ModuleSource {
            file_path: entry_path.unwrap_or("<entry>").to_owned(),
            logical_module: None,
            source: s,
        });
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
            Self::IrValidate { bag, .. } => write!(f, "{bag}"),
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
            Self::IrValidate { bag, .. } => Some(bag),
            Self::Codegen(e) => Some(e),
            Self::Io(e) => Some(e),
        }
    }
}

/// Parses and resolves Phoenix `source` as a **single compilation unit** (no crate loader).
///
/// `#import` is not supported: resolution uses [`resolve`] on one file only, so imports fail with
/// [`phx_diagnostics::ResolveError::ImportNotSupported`]. For multi-file programs use
/// [`compile_source_with_module_root`], [`check_file_with_module_path`], or [`crate::build_project`].
///
/// # Errors
///
/// Returns [`CompileError::Parse`] or [`CompileError::Resolve`] on failure.
pub fn compile_source(source: &str, path: Option<&Path>) -> Result<CompilationUnit, CompileError> {
    let parsed = parse(source);
    let prior_parse = parsed.has_errors().then(|| Box::new(parsed.errors_bag()));
    let mut source_file = parsed.value;
    if let Err(err) = strip_cfg(
        &mut source_file.program,
        &CompileCfg::host(),
        &source_file.interner,
    ) {
        let mut bag = DiagnosticBag::new();
        bag.push(
            0,
            phx_diagnostics::ResolveError::InvalidCfg {
                span: err.span,
                message: err.message,
            },
        );
        return Err(CompileError::Resolve {
            bag,
            context: None,
            prior_parse,
        });
    }
    if let Err(err) = expand_derives(&mut source_file.program, &source_file.interner) {
        let mut bag = DiagnosticBag::new();
        bag.push(
            0,
            phx_diagnostics::ResolveError::InvalidCfg {
                span: err.span,
                message: format!("invalid `#derive`: {}", err.message),
            },
        );
        return Err(CompileError::Resolve {
            bag,
            context: None,
            prior_parse,
        });
    }
    let resolved = resolve(&source_file).map_err(|bag| CompileError::Resolve {
        bag,
        context: None,
        prior_parse: prior_parse.clone(),
    })?;
    let ctx = DiagnosticContext::from_resolved(&resolved);
    let typed = type_check(resolved).map_err(|bag| CompileError::TypeCheck {
        bag,
        context: ctx,
        prior_parse: prior_parse.clone(),
    })?;
    if let Some(bag) = prior_parse {
        return Err(CompileError::Parse(*bag));
    }
    Ok(CompilationUnit {
        path: path.map(Path::to_path_buf),
        source: source.to_owned(),
        typed,
    })
}

/// Runs the lint pass on a type-checked program.
///
/// # Errors
///
/// Returns [`DiagnosticBag`] when `#[allow(...)]` names are invalid.
pub fn lint_checked(typed: &crate::typeck::TypedProgram) -> Result<LintBag, DiagnosticBag> {
    lint_program(typed)
}

/// Formats lint warnings for stderr (does not fail the build).
#[must_use]
pub fn format_lints(
    lints: &LintBag,
    context: &DiagnosticContext,
    style: &dyn DiagnosticStyle,
) -> String {
    let rows: Vec<(u32, String)> = context
        .modules
        .iter()
        .map(|m| {
            (
                m.id,
                if m.filesystem.as_os_str().is_empty() {
                    m.logical_path.clone()
                } else {
                    phx_diagnostics::diagnostic_display_path(&m.filesystem)
                },
            )
        })
        .collect();
    let module_rows: Vec<(u32, &str, &str)> = context
        .modules
        .iter()
        .zip(rows.iter())
        .map(|(m, (_, path))| (m.id, m.source.as_ref(), path.as_str()))
        .collect();
    format_lints_styled(lints, &module_rows, style)
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
    let Some(loaded) = load_program(path, module_root, &mut bag) else {
        return Err(CompileError::Resolve {
            bag,
            context: None,
            prior_parse: None,
        });
    };
    let ctx = DiagnosticContext::from_loaded(&loaded.modules, loaded.interner.clone());
    let resolved = resolve_loaded_program(loaded).map_err(|bag| CompileError::Resolve {
        bag,
        context: Some(ctx.clone()),
        prior_parse: None,
    })?;
    let typeck_ctx = DiagnosticContext::from_resolved(&resolved);
    let typed = type_check(resolved).map_err(|bag| CompileError::TypeCheck {
        bag,
        context: typeck_ctx,
        prior_parse: None,
    })?;
    Ok(CompilationUnit {
        path: Some(path.to_path_buf()),
        source: source.to_owned(),
        typed,
    })
}

/// Reads `path` and runs the full front-end (multi-file when `#import` is used).
///
/// When `path` lies under a tree with `phoenix.toml`, uses project `module_src` and path
/// dependencies (same as `phx build`). Otherwise module root defaults to `path.parent()` (or `"."`
/// if missing). Pass an explicit root via [`check_file_with_module_path`] or CLI `--module-src`.
///
/// # Errors
///
/// Returns I/O errors, [`CompileError::Parse`], [`CompileError::Resolve`], or [`CompileError::TypeCheck`].
pub fn check_file(path: &Path) -> Result<CompilationUnit, CompileError> {
    if let Ok(config) = crate::project::discover_project(path) {
        return check_project_file(path, &config);
    }
    check_file_with_module_path(path, path.parent().unwrap_or(Path::new(".")))
}

/// Type-checks `path` as part of a [`ProjectConfig`] crate (imports, deps, `.pxi` surfaces).
///
/// # Errors
///
/// Same as [`check_file`].
pub fn check_project_file(
    path: &Path,
    config: &ProjectConfig,
) -> Result<CompilationUnit, CompileError> {
    let source = std::fs::read_to_string(path).map_err(CompileError::Io)?;
    let ctx = ProgramLoadContext::from_config(config);
    let layout = BuildLayout::new(config);
    let mut bag = DiagnosticBag::new();
    let Some(loaded) = load_program_with_context(path, &ctx, Some(&layout), &mut bag) else {
        return Err(CompileError::Resolve {
            bag,
            context: None,
            prior_parse: None,
        });
    };
    let ctx_diag = DiagnosticContext::from_loaded(&loaded.modules, loaded.interner.clone());
    let resolved = resolve_loaded_program(loaded).map_err(|bag| CompileError::Resolve {
        bag,
        context: Some(ctx_diag.clone()),
        prior_parse: None,
    })?;
    let typeck_ctx = DiagnosticContext::from_resolved(&resolved);
    let typed = type_check(resolved).map_err(|bag| CompileError::TypeCheck {
        bag,
        context: typeck_ctx,
        prior_parse: None,
    })?;
    Ok(CompilationUnit {
        path: Some(path.to_path_buf()),
        source,
        typed,
    })
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
    let Some(loaded) = load_program(path, module_root, &mut bag) else {
        let context = None;
        return Err(CompileError::Resolve {
            bag,
            context,
            prior_parse: None,
        });
    };
    let ctx = DiagnosticContext::from_loaded(&loaded.modules, loaded.interner.clone());
    let resolved = resolve_loaded_program(loaded).map_err(|bag| CompileError::Resolve {
        bag,
        context: Some(ctx.clone()),
        prior_parse: None,
    })?;
    let typeck_ctx = DiagnosticContext::from_resolved(&resolved);
    let typed = type_check(resolved).map_err(|bag| CompileError::TypeCheck {
        bag,
        context: typeck_ctx,
        prior_parse: None,
    })?;
    Ok(CompilationUnit {
        path: Some(path.to_path_buf()),
        source,
        typed,
    })
}

/// Lowers and emits bytecode for an already type-checked [`CompilationUnit`].
///
/// # Errors
///
/// Returns [`CompileError::Lower`], [`CompileError::IrValidate`], or [`CompileError::Codegen`].
pub fn compile_compilation_unit(unit: &CompilationUnit) -> Result<BytecodeModule, CompileError> {
    let ctx = DiagnosticContext::from_resolved(&unit.typed.resolved);
    let ir = lower(&unit.typed).map_err(|bag| CompileError::Lower {
        bag,
        context: ctx.clone(),
    })?;
    debug_validate_ir(&ir, &unit.typed, ctx)?;
    codegen(&ir, &unit.typed).map_err(CompileError::Codegen)
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
    compile_compilation_unit(&unit)
}

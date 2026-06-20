//! `phx check` handler — type-check without codegen.
//!
//! This module is the entry point for [`run_check`]. It resolves project vs
//! standalone workflow via [`crate::workflow::resolve_check_mode`], loads and
//! resolves the program graph, runs the type checker, optionally emits
//! interface artifacts, and applies lint deny policy before reporting success.
//!
//! ```text
//! FileCommandArgs + <file.phx>
//!       │
//!       ▼
//! resolve_check_mode ──► CompileMode::Project | Standalone
//!       │
//!       ▼
//! load_program_with_context ──► resolve_loaded_program ──► type_check
//!       │                              │                        │
//!       └──────── CompileError ────────┴────────────────────────┘
//!       │
//!       ├── (optional) emit_interfaces_from_compiled  [--emit-interface-only]
//!       │
//!       ▼
//! lint_typed_or_exit
//!       │
//!       ▼
//! Reporter::check_finished ──► CliExit::Ok
//! ```
//!
//! ## Project vs standalone
//!
//! **Project mode** discovers `phoenix.toml`, loads the module graph with
//! [`BuildLayout`], and uses `phoenix.toml` `[lint] deny` for lint policy.
//! **Standalone mode** compiles a single entry file using module flags from
//! [`crate::args::FileCommandArgs`]; lint policy defaults to warn-only unless
//! `--deny` is passed.
//!
//! The `--emit-interface-only` flag requires project mode: it reloads the
//! program and writes interface files via
//! [`phx_compiler::emit_interfaces_from_compiled`] without producing bytecode.
//!
//! ## Compiler passes invoked
//!
//! Load (parse + module resolution), resolve, type check, optional interface
//! emit, and the lint pass ([`crate::lints::lint_typed_or_exit`]). No IR
//! lowering, codegen, bytecode verification, or VM execution.
//!
//! ## Exit codes
//!
//! | Outcome | [`crate::exit::CliExit`] |
//! | --- | --- |
//! | Success | [`CliExit::Ok`] |
//! | Missing file, workflow violation, bad `--emit-interface-only`, standalone load context | [`CliExit::Usage`] |
//! | Source read failure | [`CliExit::Io`] |
//! | Load/resolve/type-check/interface emit/lint deny | [`CliExit::Compile`] |
//!
//! ## Public entry points
//!
//! - [`run_check`] — run `phx check` for the given file and flags.

use std::fs;
use std::time::Instant;

use phx_compiler::{
    BuildLayout, BuildOptions, BuildProfile, CompileError, DiagnosticContext, ProgramLoadContext,
    emit_interfaces_from_compiled, load_program_with_context, resolve_loaded_program,
    unstable::type_check,
};
use phx_diagnostics::DiagnosticBag;

use phx_diagnostics::LintDenyConfig;

use crate::args::{FileCommandArgs, effective_lint_deny};
use crate::color::ColorChoice;
use crate::exit::CliExit;
use crate::lints::lint_typed_or_exit;
use crate::report::Reporter;
use crate::workflow::{CompileMode, resolve_check_mode};

/// Runs `phx check <file>` — type-check the entry file without codegen.
///
/// Resolves [`CompileMode`] from the entry path and [`FileCommandArgs`], reads
/// source from disk, then runs load → resolve → type check. When
/// `file_args.emit_interface_only` is set, emits interface artifacts for a
/// project (see module docs). Finally runs the lint pass and prints a summary
/// via [`Reporter::check_finished`].
///
/// # Errors
///
/// Returns a non-[`CliExit::Ok`] variant instead of panicking:
///
/// - [`CliExit::Usage`] — missing `<file.phx>`, workflow resolution failure,
///   `--emit-interface-only` outside a project, or standalone load-context error.
/// - [`CliExit::Io`] — cannot read the entry source file.
/// - [`CliExit::Compile`] — load, resolve, type-check, interface emit, or lint
///   deny policy failure (diagnostics rendered through `reporter`).
///
/// # Panics
///
/// Never panics on user input or malformed source.
#[allow(clippy::too_many_lines)] // project vs single-file branches + optional interface emit
pub fn run_check(file_args: FileCommandArgs, color: ColorChoice, verbose: bool) -> CliExit {
    let style = crate::color::diagnostic_style(color);
    let reporter = Reporter::new(&style);

    let file = match &file_args.file {
        Some(f) => f.clone(),
        None => {
            reporter.usage_error("missing required argument <file.phx>");
            return CliExit::Usage;
        }
    };

    let mode = match resolve_check_mode(&file, &file_args, None) {
        Ok(m) => m,
        Err(e) => {
            reporter.usage_error(&e.to_string());
            return CliExit::Usage;
        }
    };

    reporter.verbose(verbose, &format!("checking {}", file.display()));

    let source = match fs::read_to_string(&file) {
        Ok(s) => s,
        Err(e) => {
            reporter.io_error(&e.to_string());
            return CliExit::Io;
        }
    };

    let started = Instant::now();
    let mut bag = DiagnosticBag::new();
    let loaded = match &mode {
        CompileMode::Project { config } => {
            let ctx = ProgramLoadContext::from_config(config);
            let layout = BuildLayout::new(config);
            load_program_with_context(&file, &ctx, Some(&layout), &mut bag)
        }
        CompileMode::Standalone { options } => {
            let ctx = match options.load_context() {
                Ok(c) => c,
                Err(e) => {
                    reporter.project_error(&e.to_string());
                    return CliExit::Usage;
                }
            };
            load_program_with_context(&options.entry, &ctx, None, &mut bag)
        }
    };

    let Some(loaded) = loaded else {
        let err = CompileError::Resolve {
            bag,
            context: None,
            prior_parse: None,
        };
        reporter.compile_error(&err, Some(&source), Some(&file));
        return CliExit::Compile;
    };

    report_loaded_modules(&reporter, &loaded.modules);
    let ctx_diag = DiagnosticContext::from_loaded(&loaded.modules, loaded.interner.clone());
    let resolved = match resolve_loaded_program(loaded) {
        Ok(resolved) => resolved,
        Err(resolve_bag) => {
            let err = CompileError::Resolve {
                bag: resolve_bag,
                context: Some(ctx_diag),
                prior_parse: None,
            };
            reporter.compile_error(&err, Some(&source), Some(&file));
            return CliExit::Compile;
        }
    };

    let module_count = resolved.modules.len();
    let typeck_ctx = DiagnosticContext::from_resolved(&resolved);
    let typed = match type_check(resolved) {
        Ok(typed) => typed,
        Err(type_bag) => {
            let err = CompileError::TypeCheck {
                bag: type_bag,
                context: typeck_ctx,
                prior_parse: None,
            };
            reporter.compile_error(&err, Some(&source), Some(&file));
            return CliExit::Compile;
        }
    };

    if file_args.emit_interface_only {
        let CompileMode::Project { ref config } = mode else {
            reporter.usage_error(
                "`--emit-interface-only` requires a phoenix.toml project (file under module_src)",
            );
            return CliExit::Usage;
        };
        let ctx = ProgramLoadContext::from_config(config);
        let layout = BuildLayout::new(config);
        let mut reload_bag = DiagnosticBag::new();
        let Some(loaded) = load_program_with_context(&file, &ctx, Some(&layout), &mut reload_bag)
        else {
            let err = CompileError::Resolve {
                bag: reload_bag,
                context: None,
                prior_parse: None,
            };
            reporter.compile_error(&err, Some(&source), Some(&file));
            return CliExit::Compile;
        };
        let options = BuildOptions {
            force: false,
            emit_interface_only: true,
            profile: BuildProfile::Dev,
        };
        match emit_interfaces_from_compiled(config, &loaded, &typed, options, None) {
            Ok(result) => {
                reporter.success(&format!(
                    "wrote interfaces to {}",
                    result.output_path.display()
                ));
            }
            Err(e) => {
                reporter.build_error(&e);
                return CliExit::Compile;
            }
        }
    }

    if let Err(exit) = lint_typed_or_exit(
        &typed,
        &lint_deny_for_mode(&mode, &file_args),
        &style,
        &reporter,
        Some(&source),
        Some(&file),
    ) {
        return exit;
    }

    reporter.check_finished(module_count, started.elapsed());
    CliExit::Ok
}

fn lint_deny_for_mode(mode: &CompileMode, file_args: &FileCommandArgs) -> LintDenyConfig {
    match mode {
        CompileMode::Project { config } => {
            effective_lint_deny(file_args.lint_deny.as_ref(), &config.lint_deny)
        }
        CompileMode::Standalone { .. } => {
            effective_lint_deny(file_args.lint_deny.as_ref(), &LintDenyConfig::warn_only())
        }
    }
}

fn report_loaded_modules(reporter: &Reporter<'_>, modules: &[phx_compiler::LoadedModule]) {
    for module in modules {
        reporter.checking_module(&module.logical_path.display(), &module.filesystem);
    }
}

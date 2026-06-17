//! `phx check` handler.

use std::fs;
use std::time::Instant;

use phx_compiler::{
    BuildLayout, BuildOptions, CompileError, DiagnosticContext, ProgramLoadContext,
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

/// Runs `phx check`.
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

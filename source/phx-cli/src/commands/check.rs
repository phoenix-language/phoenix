//! `phx check` handler.

use std::fs;
use std::time::Instant;

use phx_compiler::{
    BuildLayout, CompileError, CrateLoadContext, DiagnosticContext, load_crate_with_context,
    resolve_crate, type_check,
};
use phx_diagnostics::DiagnosticBag;

use crate::args::FileCommandArgs;
use crate::color::ColorChoice;
use crate::exit::CliExit;
use crate::report::Reporter;
use crate::workflow::{CompileMode, resolve_check_mode};

/// Runs `phx check`.
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
            let ctx = CrateLoadContext::from_config(config);
            let layout = BuildLayout::new(config);
            load_crate_with_context(&file, &ctx, Some(&layout), &mut bag)
        }
        CompileMode::Standalone { options } => {
            let ctx = match options.load_context() {
                Ok(c) => c,
                Err(e) => {
                    reporter.project_error(&e.to_string());
                    return CliExit::Usage;
                }
            };
            load_crate_with_context(&options.entry, &ctx, None, &mut bag)
        }
    };

    let Some(loaded) = loaded else {
        let err = CompileError::Resolve { bag, context: None };
        reporter.compile_error(&err, Some(&source), Some(&file));
        return CliExit::Compile;
    };

    report_loaded_modules(&reporter, &loaded.modules);
    let ctx_diag = DiagnosticContext::from_loaded(&loaded.modules, loaded.interner.clone());
    let resolved = match resolve_crate(loaded) {
        Ok(resolved) => resolved,
        Err(resolve_bag) => {
            let err = CompileError::Resolve {
                bag: resolve_bag,
                context: Some(ctx_diag),
            };
            reporter.compile_error(&err, Some(&source), Some(&file));
            return CliExit::Compile;
        }
    };

    let module_count = resolved.modules.len();
    if let Err(type_bag) = type_check(&resolved) {
        let err = CompileError::TypeCheck {
            bag: type_bag,
            context: DiagnosticContext::from_resolved(&resolved),
        };
        reporter.compile_error(&err, Some(&source), Some(&file));
        return CliExit::Compile;
    }

    reporter.check_finished(module_count, started.elapsed());
    CliExit::Ok
}

fn report_loaded_modules(reporter: &Reporter<'_>, modules: &[phx_compiler::LoadedModule]) {
    for module in modules {
        reporter.checking_module(&module.logical_path.display(), &module.filesystem);
    }
}

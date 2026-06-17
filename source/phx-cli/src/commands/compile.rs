//! `phx compile` handler.

use std::fs;

use phx_bytecode::verify;
use phx_compiler::{check_standalone_unit_with_context, compile_compilation_unit};

use phx_diagnostics::LintDenyConfig;

use crate::args::{CompileCommandArgs, effective_lint_deny};
use crate::color::ColorChoice;
use crate::exit::CliExit;
use crate::lints::lint_typed_or_exit;
use crate::report::Reporter;
use crate::workflow::{CompileMode, resolve_check_mode};

/// Runs `phx compile`.
pub fn run_compile(args: CompileCommandArgs, color: ColorChoice, verbose: bool) -> CliExit {
    let style = crate::color::diagnostic_style(color);
    let reporter = Reporter::new(&style);

    let file = match &args.file_args.file {
        Some(f) => f.clone(),
        None => {
            reporter.usage_error("missing required argument <file.phx>");
            return CliExit::Usage;
        }
    };
    let out = match args.output {
        Some(o) => o,
        None => {
            reporter.usage_error("missing required argument -o <output.phx0>");
            return CliExit::Usage;
        }
    };

    let mode = match resolve_check_mode(&file, &args.file_args, None) {
        Ok(CompileMode::Standalone { options }) => options,
        Ok(CompileMode::Project { .. }) => {
            reporter.usage_error(
                "phx compile does not support project mode; use `phx build` or run outside a project directory",
            );
            return CliExit::Usage;
        }
        Err(e) => {
            reporter.usage_error(&e.to_string());
            return CliExit::Usage;
        }
    };

    let source = match fs::read_to_string(&file) {
        Ok(s) => s,
        Err(e) => {
            reporter.io_error(&e.to_string());
            return CliExit::Io;
        }
    };

    let ctx = match mode.load_context() {
        Ok(c) => c,
        Err(e) => {
            reporter.project_error(&e.to_string());
            return CliExit::Usage;
        }
    };

    reporter.verbose(verbose, &format!("compiling {}...", file.display()));
    let unit = match check_standalone_unit_with_context(&mode, &ctx) {
        Ok(u) => u,
        Err(e) => {
            reporter.compile_error(&e, Some(&source), Some(&file));
            return CliExit::Compile;
        }
    };
    if let Err(exit) = lint_typed_or_exit(
        &unit.typed,
        &effective_lint_deny(
            args.file_args.lint_deny.as_ref(),
            &LintDenyConfig::warn_only(),
        ),
        &style,
        &reporter,
        Some(&source),
        Some(&file),
    ) {
        return exit;
    }
    let module = match compile_compilation_unit(&unit) {
        Ok(m) => m,
        Err(e) => {
            reporter.compile_error(&e, Some(&source), Some(&file));
            return CliExit::Compile;
        }
    };

    if let Err(e) = verify(&module) {
        reporter.verify_error(&e.to_string());
        return CliExit::Verify;
    }

    let bytes = match module.encode() {
        Ok(b) => b,
        Err(e) => {
            reporter.io_error(&e.to_string());
            return CliExit::Io;
        }
    };

    if let Err(e) = fs::write(&out, bytes) {
        reporter.io_error(&e.to_string());
        return CliExit::Io;
    }

    reporter.verbose(verbose, &format!("wrote {}", out.display()));
    CliExit::Ok
}

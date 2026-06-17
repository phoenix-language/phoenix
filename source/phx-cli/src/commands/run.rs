//! `phx run` handler.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use phx_bytecode::verify;
use phx_compiler::{
    BuildOptions, build_project, check_standalone_unit_with_context, compile_compilation_unit,
    load_project_binary,
};
use phx_diagnostics::DiagnosticStyle;
use phx_vm::{
    DEFAULT_HEAP_CAP_BYTES, Value, register_builtin_foreign_stubs, run_captured_with_heap_cap,
    run_with_heap_cap,
};

use crate::args::RunCommandArgs;
use crate::color::ColorChoice;
use crate::exit::CliExit;
use crate::lints::{emit_lint_warnings, lint_typed_or_exit};
use crate::report::Reporter;
use crate::workflow::{CompileMode, resolve_run_mode};

/// Runs `phx run`.
pub fn run_run(
    args: RunCommandArgs,
    color: ColorChoice,
    verbose: bool,
    dump_main: bool,
) -> CliExit {
    let style = crate::color::diagnostic_style(color);
    let reporter = Reporter::new(&style);

    let mode = match resolve_run_mode(
        args.file_args.file.as_deref(),
        &args.file_args,
        args.project_root.as_deref(),
    ) {
        Ok(m) => m,
        Err(e) => {
            reporter.usage_error(&e.to_string());
            return CliExit::Usage;
        }
    };

    match mode {
        CompileMode::Project { config } => run_project(
            &reporter,
            &style,
            &config,
            args.file_args.file.as_deref(),
            args.force_build,
            args.skip_build,
            verbose,
            dump_main,
            resolve_heap_cap(args.heap_cap, Some(&config)),
        ),
        CompileMode::Standalone { options } => run_standalone(
            &reporter,
            &style,
            &options,
            verbose,
            dump_main,
            resolve_heap_cap(args.heap_cap, None),
        ),
    }
}

fn resolve_heap_cap(cli: Option<usize>, config: Option<&phx_compiler::ProjectConfig>) -> usize {
    if let Some(cap) = cli {
        return cap;
    }
    if let Some(config) = config {
        if let Some(cap) = config.vm_heap_cap_bytes {
            return cap;
        }
    }
    DEFAULT_HEAP_CAP_BYTES
}

#[allow(clippy::fn_params_excessive_bools, clippy::too_many_arguments)]
fn run_project(
    reporter: &Reporter<'_>,
    style: &dyn DiagnosticStyle,
    config: &phx_compiler::ProjectConfig,
    entry: Option<&Path>,
    force: bool,
    skip_build: bool,
    verbose: bool,
    dump_main: bool,
    heap_cap: usize,
) -> CliExit {
    if !skip_build {
        reporter.verbose(verbose, "building project...");
        let options = BuildOptions {
            force,
            emit_interface_only: false,
        };
        match build_project(config, entry, options) {
            Ok(result) => {
                if let Some(ctx) = &result.lint_context {
                    emit_lint_warnings(&result.lints, ctx, style);
                }
            }
            Err(e) => {
                reporter.build_error(&e);
                return CliExit::Compile;
            }
        }
    }
    reporter.verbose(verbose, "loading bytecode...");
    let module = match load_project_binary(config) {
        Ok(m) => m,
        Err(e) => {
            reporter.build_error(&e);
            return CliExit::Compile;
        }
    };
    execute_module(reporter, &module, verbose, dump_main, heap_cap)
}

fn run_standalone(
    reporter: &Reporter<'_>,
    style: &dyn DiagnosticStyle,
    options: &phx_compiler::StandaloneOptions,
    verbose: bool,
    dump_main: bool,
    heap_cap: usize,
) -> CliExit {
    let source = match fs::read_to_string(&options.entry) {
        Ok(s) => s,
        Err(e) => {
            reporter.io_error(&e.to_string());
            return CliExit::Io;
        }
    };
    let ctx = match options.load_context() {
        Ok(c) => c,
        Err(e) => {
            reporter.project_error(&e.to_string());
            return CliExit::Usage;
        }
    };
    reporter.verbose(
        verbose,
        &format!("compiling {}...", options.entry.display()),
    );
    let unit = match check_standalone_unit_with_context(options, &ctx) {
        Ok(u) => u,
        Err(e) => {
            reporter.compile_error(&e, Some(&source), Some(&options.entry));
            return CliExit::Compile;
        }
    };
    if let Err(exit) = lint_typed_or_exit(
        &unit.typed,
        style,
        reporter,
        Some(&source),
        Some(&options.entry),
    ) {
        return exit;
    }
    let module = match compile_compilation_unit(&unit) {
        Ok(m) => m,
        Err(e) => {
            reporter.compile_error(&e, Some(&source), Some(&options.entry));
            return CliExit::Compile;
        }
    };
    execute_module(reporter, &module, verbose, dump_main, heap_cap)
}

fn execute_module(
    reporter: &Reporter<'_>,
    module: &phx_bytecode::BytecodeModule,
    verbose: bool,
    dump_main: bool,
    heap_cap: usize,
) -> CliExit {
    reporter.verbose(verbose, "verifying bytecode...");
    let verified = match verify(module) {
        Ok(verified) => verified,
        Err(e) => {
            reporter.verify_error(&e.to_string());
            return CliExit::Verify;
        }
    };
    reporter.verbose(verbose, "running...");
    register_builtin_foreign_stubs();
    if dump_main {
        match run_captured_with_heap_cap(verified, heap_cap) {
            Ok(capture) => {
                dump_main_locals(capture.main_locals.as_slice());
                CliExit::Ok
            }
            Err(e) => {
                reporter.runtime_error(&e.to_string());
                CliExit::Runtime
            }
        }
    } else if let Err(e) = run_with_heap_cap(verified, heap_cap) {
        reporter.runtime_error(&e.to_string());
        CliExit::Runtime
    } else {
        CliExit::Ok
    }
}

fn dump_main_locals(locals: &[Value]) {
    for (index, value) in locals.iter().enumerate() {
        let mut line = format!("main[{index}]: ");
        let _ = write!(line, "{value:?}");
        eprintln!("{line}");
    }
}

//! `phx run` handler.

use std::fs;
use std::path::Path;

use phx_bytecode::verify;
use phx_compiler::{build_project, compile_standalone_with_context, load_project_binary};

use crate::args::RunCommandArgs;
use crate::color::ColorChoice;
use crate::exit::CliExit;
use crate::report::Reporter;
use crate::workflow::{CompileMode, resolve_run_mode};

/// Runs `phx run`.
pub fn run_run(args: RunCommandArgs, color: ColorChoice, verbose: bool) -> CliExit {
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
            &config,
            args.file_args.file.as_deref(),
            args.force_build,
            args.skip_build,
            verbose,
        ),
        CompileMode::Standalone { options } => run_standalone(&reporter, &options, verbose),
    }
}

fn run_project(
    reporter: &Reporter<'_>,
    config: &phx_compiler::ProjectConfig,
    entry: Option<&Path>,
    force: bool,
    skip_build: bool,
    verbose: bool,
) -> CliExit {
    if !skip_build {
        reporter.verbose(verbose, "building project...");
        if let Err(e) = build_project(config, entry, force) {
            reporter.build_error(&e);
            return CliExit::Compile;
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
    execute_module(reporter, &module, verbose)
}

fn run_standalone(
    reporter: &Reporter<'_>,
    options: &phx_compiler::StandaloneOptions,
    verbose: bool,
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
    let module = match compile_standalone_with_context(options, &ctx) {
        Ok(m) => m,
        Err(e) => {
            reporter.compile_error(&e, Some(&source));
            return CliExit::Compile;
        }
    };
    execute_module(reporter, &module, verbose)
}

fn execute_module(
    reporter: &Reporter<'_>,
    module: &phx_bytecode::BytecodeModule,
    verbose: bool,
) -> CliExit {
    reporter.verbose(verbose, "verifying bytecode...");
    if let Err(e) = verify(module) {
        reporter.verify_error(&e.to_string());
        return CliExit::Verify;
    }
    reporter.verbose(verbose, "running...");
    if let Err(e) = phx_vm::run(module) {
        reporter.runtime_error(&e.to_string());
        return CliExit::Runtime;
    }
    CliExit::Ok
}

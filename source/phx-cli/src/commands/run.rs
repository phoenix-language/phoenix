//! `phx run` handler — compile when needed and execute on the VM.
//!
//! This module is the entry point for [`run_run`]. It resolves project vs
//! standalone workflow via [`crate::workflow::resolve_run_mode`], produces or
//! loads bytecode, verifies it, and executes the program entry on the VM with
//! an optional heap cap and `--dump-main` local capture.
//!
//! ```text
//! RunCommandArgs
//!       │
//!       ▼
//! resolve_run_mode ──► CompileMode::Project | Standalone
//!       │
//!       ├── Project ──► build_project (unless --skip-build)
//!       │                    │
//!       │                    ▼
//!       │              load_project_binary
//!       │
//!       └── Standalone ──► check_standalone_unit_with_context
//!                               │
//!                               ▼
//!                         compile_compilation_unit
//!       │
//!       ▼
//! verify ──► run_with_heap_cap | run_captured_with_heap_cap
//!       │
//!       ▼
//! CliExit::Ok | Runtime | Verify | …
//! ```
//!
//! ## Project vs standalone
//!
//! **Project mode** optionally runs [`phx_compiler::build_project`] (unless
//! `--skip-build`), loads the built binary from the project output layout, and
//! applies lint policy from `phoenix.toml` after the build. **Standalone mode**
//! type-checks and compiles the entry file in-process via
//! [`phx_compiler::check_standalone_unit_with_context`] and
//! [`phx_compiler::compile_compilation_unit`], with warn-only lint policy unless
//! `--deny` is passed.
//!
//! Heap capacity comes from `--heap-cap`, then `phoenix.toml` `vm_heap_cap_bytes`,
//! then [`phx_vm::DEFAULT_HEAP_CAP_BYTES`].
//!
//! ## Compiler passes invoked
//!
//! **Project:** full project build (load, resolve, type check, lint, lower,
//! codegen) when build is not skipped, then bytecode load from disk.
//! **Standalone:** load, resolve, type check, lint, lower, codegen in one unit.
//! Both paths run [`phx_bytecode::verify`] before VM execution.
//!
//! ## Exit codes
//!
//! | Outcome | [`crate::exit::CliExit`] |
//! | --- | --- |
//! | Success | [`CliExit::Ok`] |
//! | Workflow resolution, standalone load context | [`CliExit::Usage`] |
//! | Standalone source read failure | [`CliExit::Io`] |
//! | Build, check, compile, lint deny | [`CliExit::Compile`] |
//! | Bytecode verification failure | [`CliExit::Verify`] |
//! | VM trap or runtime fault | [`CliExit::Runtime`] |
//!
//! ## Public entry points
//!
//! - [`run_run`] — run `phx run` for the given arguments and global flags.

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

use phx_diagnostics::LintDenyConfig;

use crate::args::{RunCommandArgs, effective_lint_deny};
use crate::color::ColorChoice;
use crate::exit::CliExit;
use crate::lints::{apply_lint_deny_policy, emit_lint_warnings, lint_typed_or_exit};
use crate::report::Reporter;
use crate::vm_diag::SourceContext;
use crate::workflow::{CompileMode, resolve_run_mode};

/// Runs `phx run` — build or compile the program, verify bytecode, and execute.
///
/// Dispatches on [`CompileMode`] from [`resolve_run_mode`]. Project invocations
/// call [`build_project`] unless `args.skip_build`, then load bytecode from the
/// project artifact path. Standalone invocations compile the entry file in memory.
/// Both paths verify bytecode and run the VM with heap cap from CLI, project
/// config, or the VM default.
///
/// When `dump_main` is true, captures and prints `main` local slots instead of
/// streaming program output normally.
///
/// # Errors
///
/// Returns a non-[`CliExit::Ok`] variant instead of panicking:
///
/// - [`CliExit::Usage`] — workflow resolution or standalone load-context failure.
/// - [`CliExit::Io`] — cannot read the standalone entry source file.
/// - [`CliExit::Compile`] — build, type-check, codegen, or lint deny failure.
/// - [`CliExit::Verify`] — bytecode fails structural verification before run.
/// - [`CliExit::Runtime`] — VM execution error (source-mapped when available).
///
/// # Panics
///
/// Never panics on user input or malformed source.
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
        CompileMode::Project { config } => {
            let deny = effective_lint_deny(args.file_args.lint_deny.as_ref(), &config.lint_deny);
            run_project(
                &reporter,
                &style,
                &config,
                args.file_args.file.as_deref(),
                args.force_build,
                args.skip_build,
                verbose,
                dump_main,
                resolve_heap_cap(args.heap_cap, Some(&config)),
                deny,
            )
        }
        CompileMode::Standalone { options } => {
            let deny = effective_lint_deny(
                args.file_args.lint_deny.as_ref(),
                &LintDenyConfig::warn_only(),
            );
            run_standalone(
                &reporter,
                &style,
                &options,
                verbose,
                dump_main,
                resolve_heap_cap(args.heap_cap, None),
                deny,
            )
        }
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
    deny: LintDenyConfig,
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
                    if let Err(exit) = apply_lint_deny_policy(&result.lints, &deny, reporter) {
                        return exit;
                    }
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
    let default_entry = config.default_entry_file();
    let entry_path = entry.unwrap_or(&default_entry);
    let source_ctx = SourceContext {
        project_root: Some(&config.root),
        entry_path: Some(entry_path),
        entry_source: None,
    };
    execute_module(reporter, &module, verbose, dump_main, heap_cap, source_ctx)
}

fn run_standalone(
    reporter: &Reporter<'_>,
    style: &dyn DiagnosticStyle,
    options: &phx_compiler::StandaloneOptions,
    verbose: bool,
    dump_main: bool,
    heap_cap: usize,
    deny: LintDenyConfig,
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
        &deny,
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
    let source_ctx = SourceContext {
        project_root: None,
        entry_path: Some(&options.entry),
        entry_source: Some(&source),
    };
    execute_module(reporter, &module, verbose, dump_main, heap_cap, source_ctx)
}

fn execute_module(
    reporter: &Reporter<'_>,
    module: &phx_bytecode::BytecodeModule,
    verbose: bool,
    dump_main: bool,
    heap_cap: usize,
    source_ctx: SourceContext<'_>,
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
                reporter.runtime_vm_error(module, &e, &source_ctx);
                CliExit::Runtime
            }
        }
    } else if let Err(e) = run_with_heap_cap(verified, heap_cap) {
        reporter.runtime_vm_error(module, &e, &source_ctx);
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

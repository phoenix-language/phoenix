//! Phoenix command-line interface library for the `phx` binary.
//!
//! This crate is the programmatic entry point for the Phoenix CLI: it parses
//! arguments, dispatches subcommands, resolves project vs standalone workflows,
//! and renders cargo-style diagnostics to stderr. The thin `phx` binary crate
//! wraps [`run`] with a panic hook that reports internal compiler errors.
//!
//! ## Entry points
//!
//! - [`run`] — parse `std::env::args()` and execute the requested subcommand.
//! - [`run_with`] — same dispatch path with pre-parsed options (tests and embedders).
//! - [`parse`] — parse only; does not run the compiler or VM.
//!
//! ## Modules
//!
//! - [`args`] — global flags, subcommands, and [`args::ParseError`].
//! - [`color`] — `--color auto|always|never` and ANSI diagnostic styling.
//! - [`commands`] — subcommand handlers (`check`, `build`, `compile`, `run`, `explain`).
//! - [`exit`] — structured process exit codes ([`exit::CliExit`]).
//! - [`help`] — usage and per-command help text.
//! - [`ice`] — internal-compiler-error reporting after an unexpected panic.
//! - [`lints`] — lint warning emission and `--deny` policy.
//! - [`report`] — diagnostic rendering helpers for command handlers.
//! - [`vm_diag`] — map VM faults to Phoenix source locations via PHX0 section 5.
//! - [`workflow`] — project vs standalone mode resolution.

#![allow(
    clippy::print_stderr,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value,
    clippy::single_match_else,
    clippy::manual_let_else,
    clippy::collapsible_if,
    clippy::match_same_arms,
    clippy::unnecessary_wraps
)]

/// Global flags, subcommands, and parse errors.
pub mod args;
/// Color output preference and diagnostic styling.
pub mod color;
/// Subcommand handlers (`check`, `build`, `compile`, `run`, `explain`).
pub mod commands;
/// Structured process exit codes.
pub mod exit;
/// Usage and per-command help text.
pub mod help;
/// Internal-compiler-error reporting after an unexpected panic.
pub mod ice;
/// Lint warning emission and `--deny` policy.
pub mod lints;
/// Diagnostic rendering helpers for command handlers.
pub mod report;
/// Map VM faults to Phoenix source locations via PHX0 section 5.
pub mod vm_diag;
/// Project vs standalone mode resolution.
pub mod workflow;

use args::{Command, ParseError, parse_env_args};
use exit::CliExit;

/// Environment variable used by integration tests to force a controlled panic path.
///
/// When set, [`run`] panics before dispatch so the `phx` binary can exercise
/// the internal-compiler-error path. Not used in normal operation.
pub const TEST_FORCE_PANIC_ENV: &str = "PHX_TEST_FORCE_PANIC";

/// Runs the CLI with process arguments from [`std::env::args`].
///
/// Parses global flags and the subcommand, then dispatches to the matching
/// handler in [`commands`]. Parse failures are reported to stderr and return
/// [`CliExit::Usage`].
///
/// # Panics
///
/// Panics when [`TEST_FORCE_PANIC_ENV`] is set (integration tests only).
pub fn run() -> CliExit {
    assert!(
        std::env::var_os(TEST_FORCE_PANIC_ENV).is_none(),
        "integration test forced panic"
    );
    match parse_env_args() {
        Ok((opts, cmd)) => dispatch(opts, cmd),
        Err(e) => args::handle_parse_error(e, true),
    }
}

fn dispatch(opts: args::CliOptions, cmd: Command) -> CliExit {
    match cmd {
        Command::Help(sub) => {
            if let Some(s) = sub {
                help::print_command_help(s);
            } else {
                help::print_usage();
            }
            CliExit::Ok
        }
        Command::Version => {
            help::print_version();
            CliExit::Ok
        }
        Command::Explain(code) => commands::run_explain(code, opts.color),
        Command::Check(args) => commands::run_check(args, opts.color, opts.verbose),
        Command::Build(args) => commands::run_build(args, opts.color, opts.verbose),
        Command::Compile(args) => commands::run_compile(args, opts.color, opts.verbose),
        Command::Run(args) => {
            let dump_main = args.dump_main;
            commands::run_run(args, opts.color, opts.verbose, dump_main)
        }
    }
}

/// Runs the CLI with pre-parsed options (for tests and embedders).
///
/// Skips argument parsing and invokes the same dispatch path as [`run`].
/// Prefer [`parse`] followed by `run_with` when tests need to assert on parse
/// errors separately from command execution.
pub fn run_with(opts: args::CliOptions, cmd: Command) -> CliExit {
    dispatch(opts, cmd)
}

/// Parses CLI arguments without executing a subcommand.
///
/// Accepts any iterator of argument strings (typically including the program
/// name as the first element, matching [`std::env::args`]). Use with
/// [`run_with`] to drive the full pipeline from tests without spawning a process.
///
/// # Errors
///
/// Returns [`ParseError`] when global flags or subcommand arguments are invalid.
pub fn parse(
    argv: impl Iterator<Item = String>,
) -> Result<(args::CliOptions, Command), ParseError> {
    args::parse_args(argv)
}

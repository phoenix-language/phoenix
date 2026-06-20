//! Phoenix command-line interface library (`phx` binary driver).
//!
//! Parses arguments, resolves project vs standalone workflows, and reports
//! cargo-style diagnostics to stderr.

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

pub mod args;
pub mod color;
pub mod commands;
pub mod exit;
pub mod help;
pub mod lints;
pub mod report;
pub mod vm_diag;
pub mod workflow;

use args::{Command, ParseError, parse_env_args};
use exit::CliExit;

/// Environment variable used by integration tests to force a controlled panic path.
pub const TEST_FORCE_PANIC_ENV: &str = "PHX_TEST_FORCE_PANIC";

/// Runs the CLI with process arguments.
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
pub fn run_with(opts: args::CliOptions, cmd: Command) -> CliExit {
    dispatch(opts, cmd)
}

/// Parses arguments without executing (for tests).
///
/// # Errors
///
/// Returns [`ParseError`] when flags or subcommand arguments are invalid.
pub fn parse(
    argv: impl Iterator<Item = String>,
) -> Result<(args::CliOptions, Command), ParseError> {
    args::parse_args(argv)
}

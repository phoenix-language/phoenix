//! `phx build` handler — compile a project to bytecode or interface artifacts.
//!
//! This module is the entry point for [`run_build`]. It discovers and loads
//! `phoenix.toml` via [`crate::workflow::resolve_build_project`], runs the full
//! project build graph, emits lint warnings, and applies deny policy from CLI
//! flags and the project manifest.
//!
//! ```text
//! ProjectCommandArgs (+ optional entry anchor)
//!       │
//!       ▼
//! resolve_build_project ──► ProjectConfig
//!       │
//!       ▼
//! build_project (force, emit_interface_only)
//!       │
//!       ├── lint_context ──► emit_lint_warnings
//!       │                         │
//!       │                         ▼
//!       │                  apply_lint_deny_policy
//!       │
//!       ▼
//! Reporter::success ──► CliExit::Ok
//! ```
//!
//! ## Project vs standalone
//!
//! **Project only.** The anchor path defaults to `.` when `--entry` is omitted;
//! an explicit `--project-root` overrides manifest discovery. There is no
//! standalone path — single-file workflows use `phx compile` or `phx check`.
//!
//! When `emit_interface_only` is set, the build writes interface files instead
//! of the main bytecode artifact; success messaging reflects the output path
//! returned by [`phx_compiler::build_project`].
//!
//! ## Compiler passes invoked
//!
//! Full project pipeline inside [`phx_compiler::build_project`]: module load,
//! resolve, type check, lint collection, and either interface emission or
//! lower/codegen to the project output layout. No VM execution or bytecode
//! verification at the CLI layer (verification runs inside the compiler build
//! when producing bytecode).
//!
//! ## Exit codes
//!
//! | Outcome | [`crate::exit::CliExit`] |
//! | --- | --- |
//! | Success | [`CliExit::Ok`] |
//! | Project discovery or manifest load failure | [`CliExit::Usage`] |
//! | Build failure or lint deny | [`CliExit::Compile`] |
//!
//! ## Public entry points
//!
//! - [`run_build`] — run `phx build` for the given project flags.

use std::path::Path;

use phx_compiler::{BuildOptions, build_project};

use crate::args::{ProjectCommandArgs, effective_lint_deny};
use crate::color::ColorChoice;
use crate::exit::CliExit;
use crate::lints::{apply_lint_deny_policy, emit_lint_warnings};
use crate::report::Reporter;
use crate::workflow::resolve_build_project;

/// Runs `phx build` — compile a Phoenix project from `phoenix.toml`.
///
/// Resolves the project from `args.entry` (default `.`) and optional
/// `args.project_root`, then invokes [`build_project`] with force and
/// interface-only options. Lint warnings are printed to stderr; denied lints
/// promote to [`CliExit::Compile`].
///
/// # Errors
///
/// Returns a non-[`CliExit::Ok`] variant instead of panicking:
///
/// - [`CliExit::Usage`] — project cannot be discovered or `phoenix.toml` fails to load.
/// - [`CliExit::Compile`] — build pipeline failure or lint deny policy match.
///
/// # Panics
///
/// Never panics on user input or malformed project configuration.
pub fn run_build(args: ProjectCommandArgs, color: ColorChoice, verbose: bool) -> CliExit {
    let style = crate::color::diagnostic_style(color);
    let reporter = Reporter::new(&style);

    let anchor = args.entry.as_deref().unwrap_or_else(|| Path::new("."));
    let config = match resolve_build_project(anchor, args.project_root.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            reporter.project_error(&e.to_string());
            return CliExit::Usage;
        }
    };

    reporter.verbose(verbose, "building project...");
    let deny = effective_lint_deny(args.lint_deny.as_ref(), &config.lint_deny);
    let options = BuildOptions {
        force: args.force_build,
        emit_interface_only: args.emit_interface_only,
    };
    match build_project(&config, args.entry.as_deref(), options) {
        Ok(result) => {
            if let Some(ctx) = &result.lint_context {
                emit_lint_warnings(&result.lints, ctx, &style);
                if let Err(exit) = apply_lint_deny_policy(&result.lints, &deny, &reporter) {
                    return exit;
                }
            }
            if args.emit_interface_only {
                reporter.success(&format!(
                    "wrote interfaces to {}",
                    result.output_path.display()
                ));
            } else {
                reporter.success(&format!("built {}", result.output_path.display()));
            }
            CliExit::Ok
        }
        Err(e) => {
            reporter.build_error(&e);
            CliExit::Compile
        }
    }
}

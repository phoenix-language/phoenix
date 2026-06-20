//! Lint warning emission and deny policy for compiling CLI commands.
//!
//! This module sits **after type checking** in the compile pipeline for
//! `check`, `compile`, and standalone `run` paths. It runs the compiler lint
//! pass, renders warnings to stderr, and optionally promotes warnings to
//! errors when `--deny` or `phoenix.toml` `[lint] deny` matches.
//!
//! ```text
//! TypedProgram
//!       │
//!       ▼
//! lint_checked ──► LintBag ──► emit_lint_warnings (stderr)
//!       │                           │
//!       │                           ▼
//!       │                    apply_lint_deny_policy
//!       │                           │
//!       └── unknown #[allow] ───────┴──► CliExit::Compile
//! ```
//!
//! Project `build` uses [`emit_lint_warnings`] and [`apply_lint_deny_policy`]
//! directly on the aggregated lint bag from `phx_compiler::build_project`.
//! Single-unit commands use [`lint_typed_or_exit`] to run the pass and apply
//! policy in one step.
//!
//! ## Public functions
//!
//! - [`emit_lint_warnings`] — render non-empty lint bags to stderr.
//! - [`apply_lint_deny_policy`] — fail the command when denied warnings remain.
//! - [`lint_typed_or_exit`] — lint a typed program or exit with compile status.

use std::path::Path;

use phx_compiler::{
    CompileError, DiagnosticContext, format_lints, lint_checked, unstable::TypedProgram,
};
use phx_diagnostics::{DiagnosticStyle, LintBag, LintDenyConfig, count_denied_lints};

use crate::exit::CliExit;
use crate::report::Reporter;

/// Prints lint warnings to stderr when `lints` is non-empty.
///
/// Formats each lint through [`format_lints`] with `ctx` for module paths and
/// source carets. No output is written when the bag is empty or formatting
/// produces an empty string.
pub fn emit_lint_warnings(lints: &LintBag, ctx: &DiagnosticContext, style: &dyn DiagnosticStyle) {
    if lints.has_lints() {
        let rendered = format_lints(lints, ctx, style);
        if !rendered.is_empty() {
            eprintln!("{rendered}");
        }
    }
}

/// Applies `deny` to an existing lint bag; returns [`CliExit::Compile`] when denied warnings remain.
///
/// Counts lints matching `deny` via [`count_denied_lints`]. When the count is
/// non-zero, emits a summary through `reporter` and returns
/// [`CliExit::Compile`] so the process exit code distinguishes lint policy
/// failures from other compile errors.
///
/// # Errors
///
/// Returns [`CliExit::Compile`] when one or more lints match the deny policy.
pub fn apply_lint_deny_policy(
    lints: &LintBag,
    deny: &LintDenyConfig,
    reporter: &Reporter<'_>,
) -> Result<(), CliExit> {
    let denied = count_denied_lints(lints, deny);
    if denied == 0 {
        return Ok(());
    }
    let noun = if denied == 1 { "warning" } else { "warnings" };
    reporter.compile_message(&format!(
        "denied {denied} {noun} due to lint policy (--deny or phoenix.toml [lint] deny)"
    ));
    Err(CliExit::Compile)
}

/// Runs the lint pass on `typed`, printing warnings or reporting invalid `#[allow]` names.
///
/// On success, warnings are emitted with [`emit_lint_warnings`] and the deny
/// policy is enforced with [`apply_lint_deny_policy`]. When `lint_checked`
/// returns a diagnostic bag (for example an unknown lint name in `#[allow(...)]`),
/// the error is rendered through `reporter` as a resolve-style
/// [`CompileError`] and the function returns [`CliExit::Compile`].
///
/// # Errors
///
/// Returns [`CliExit::Compile`] when `#[allow(...)]` uses an unknown lint name or when `deny`
/// treats a warning as an error.
pub fn lint_typed_or_exit(
    typed: &TypedProgram,
    deny: &LintDenyConfig,
    style: &dyn DiagnosticStyle,
    reporter: &Reporter<'_>,
    source: Option<&str>,
    file: Option<&Path>,
) -> Result<(), CliExit> {
    match lint_checked(typed) {
        Ok(lint_bag) => {
            let ctx = DiagnosticContext::from_resolved(&typed.resolved);
            emit_lint_warnings(&lint_bag, &ctx, style);
            apply_lint_deny_policy(&lint_bag, deny, reporter)
        }
        Err(bag) => {
            let err = CompileError::Resolve {
                bag,
                context: Some(DiagnosticContext::from_resolved(&typed.resolved)),
                prior_parse: None,
            };
            reporter.compile_error(&err, source, file);
            Err(CliExit::Compile)
        }
    }
}

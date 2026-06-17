//! Lint warning emission for compiling CLI commands.

use std::path::Path;

use phx_compiler::{
    CompileError, DiagnosticContext, format_lints, lint_checked, unstable::TypedProgram,
};
use phx_diagnostics::{DiagnosticStyle, LintBag, LintDenyConfig, count_denied_lints};

use crate::exit::CliExit;
use crate::report::Reporter;

/// Prints lint warnings to stderr when `lints` is non-empty.
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

//! Lint warning emission for compiling CLI commands.

use std::path::Path;

use phx_compiler::{
    CompileError, DiagnosticContext, format_lints, lint_checked, unstable::TypedProgram,
};
use phx_diagnostics::{DiagnosticStyle, LintBag};

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

/// Runs the lint pass on `typed`, printing warnings or reporting invalid `#[allow]` names.
///
/// # Errors
///
/// Returns [`CliExit::Compile`] when `#[allow(...)]` uses an unknown lint name.
pub fn lint_typed_or_exit(
    typed: &TypedProgram,
    style: &dyn DiagnosticStyle,
    reporter: &Reporter<'_>,
    source: Option<&str>,
    file: Option<&Path>,
) -> Result<(), CliExit> {
    match lint_checked(typed) {
        Ok(lint_bag) => {
            let ctx = DiagnosticContext::from_resolved(&typed.resolved);
            emit_lint_warnings(&lint_bag, &ctx, style);
            Ok(())
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

//! Routes compiler and runtime errors to stderr.

use phx_compiler::{BuildError, CompileError};
use phx_diagnostics::DiagnosticStyle;

use crate::color::AnsiStyle;

/// Emits formatted compiler diagnostics to stderr.
pub struct Reporter<'a> {
    style: &'a dyn DiagnosticStyle,
}

impl std::fmt::Debug for Reporter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reporter").finish_non_exhaustive()
    }
}

impl<'a> Reporter<'a> {
    /// Creates a reporter with the given style.
    #[must_use]
    pub const fn new(style: &'a dyn DiagnosticStyle) -> Self {
        Self { style }
    }

    /// Reports a [`CompileError`] with source carets when available.
    pub fn compile_error(&self, err: &CompileError, entry_source: Option<&str>) {
        let (modules, interner) = match err {
            CompileError::Resolve { context, .. } => (
                context.as_ref().map(|c| c.modules.as_slice()),
                context.as_ref().map(|c| &c.interner),
            ),
            CompileError::TypeCheck { context, .. } => {
                (Some(context.modules.as_slice()), Some(&context.interner))
            }
            CompileError::Lower { context, .. } => {
                (Some(context.modules.as_slice()), Some(&context.interner))
            }
            _ => (None, None),
        };
        let msg = err.format_with_modules_styled(entry_source, modules, interner, self.style);
        eprint_line(&msg);
    }

    /// Reports a [`BuildError`].
    pub fn build_error(&self, err: &BuildError) {
        eprint_line(&self.style.plain_error(&err.to_message()));
    }

    /// Reports a project configuration error.
    pub fn project_error(&self, message: &str) {
        eprint_line(&self.style.plain_error(message));
    }

    /// Reports a verify failure.
    pub fn verify_error(&self, message: &str) {
        eprint_line(&self.style.plain_error(&format!("verify error: {message}")));
    }

    /// Reports a VM runtime failure.
    pub fn runtime_error(&self, message: &str) {
        eprint_line(&self.style.plain_error(&format!("runtime error: {message}")));
    }

    /// Reports a generic I/O failure.
    pub fn io_error(&self, message: &str) {
        eprint_line(&self.style.plain_error(&format!("I/O error: {message}")));
    }

    /// Reports a usage or workflow error.
    pub fn usage_error(&self, message: &str) {
        eprint_line(&self.style.plain_error(message));
    }

    /// Reports a success status (build output path, etc.).
    pub fn success(&self, message: &str) {
        eprint_line(&self.style.success(message));
    }

    /// Reports verbose pipeline status when enabled.
    pub fn verbose(&self, enabled: bool, message: &str) {
        if enabled {
            eprint_line(message);
        }
    }
}

/// Creates a reporter from a color choice.
#[must_use]
pub fn reporter(choice: crate::color::ColorChoice) -> AnsiStyle {
    crate::color::diagnostic_style(choice)
}

fn eprint_line(msg: &str) {
    // CLI is allowed to write user-facing diagnostics to stderr.
    eprintln!("{msg}");
}

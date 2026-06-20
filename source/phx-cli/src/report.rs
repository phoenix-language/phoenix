//! User-facing diagnostic output routed to stderr.
//!
//! This module is the **rendering layer** between command handlers and the
//! terminal: every compile failure, runtime fault, progress line, and success
//! message flows through [`Reporter`] so styling stays consistent with
//! [`crate::color::AnsiStyle`] and [`DiagnosticStyle`] from `phx-diagnostics`.
//!
//! ```text
//! command handler
//!       │
//!       ├── CompileError / BuildError ──► Reporter::compile_error / build_error
//!       │                                      │
//!       │                                      ▼
//!       │                                 eprintln! (stderr)
//!       │
//!       ├── VmError + PHX0 section 5 ──► Reporter::runtime_vm_error
//!       │                                      │
//!       │                                      ▼
//!       │                                 source-mapped runtime line
//!       │
//!       └── usage / I/O / verify ──────► Reporter::usage_error / io_error / …
//! ```
//!
//! ## Compile vs runtime errors
//!
//! **Compile-time** paths use structured errors from `phx-compiler`:
//! [`CompileError`] carries a diagnostic bag and optional [`DiagnosticContext`]
//! so carets and module paths render correctly; [`BuildError`] and plain
//! strings cover project layout and workflow failures. Handlers call
//! [`Reporter::compile_error`] when a full diagnostic is available and
//! [`Reporter::compile_message`] for summary-only failures.
//!
//! **Runtime** paths use either a plain message ([`Reporter::runtime_error`])
//! or [`Reporter::runtime_vm_error`], which maps a [`VmError`] through
//! [`crate::vm_diag::format_vm_error`] when the loaded bytecode module
//! includes PHX0 section 5 source mapping.
//!
//! ## Public types
//!
//! - [`Reporter`] — styled stderr reporter bound to an [`AnsiStyle`].
//!
//! ## Entry points
//!
//! - [`Reporter::new`] — construct a reporter from a resolved style.
//! - [`reporter`] — convenience wrapper that builds an [`AnsiStyle`] from
//!   [`crate::color::ColorChoice`].

use std::path::Path;
use std::time::Duration;

use phx_bytecode::BytecodeModule;
use phx_compiler::{BuildError, CompileError};
use phx_diagnostics::DiagnosticStyle;
use phx_vm::VmError;

use crate::vm_diag::{SourceContext, format_vm_error};

use crate::color::AnsiStyle;

/// Emits formatted compiler, runtime, and workflow messages to stderr.
///
/// Command handlers construct one [`Reporter`] per invocation from
/// [`crate::color::diagnostic_style`] and reuse it for every failure or
/// progress line in that run. All methods write to stderr via [`eprintln!`];
/// stdout remains free for program output (`phx run`) and machine-readable
/// artifacts.
pub struct Reporter<'a> {
    style: &'a AnsiStyle,
}

impl std::fmt::Debug for Reporter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reporter").finish_non_exhaustive()
    }
}

impl<'a> Reporter<'a> {
    /// Creates a reporter that applies `style` to every emitted line.
    #[must_use]
    pub const fn new(style: &'a AnsiStyle) -> Self {
        Self { style }
    }

    /// Reports a [`CompileError`] with source carets and module paths when available.
    ///
    /// Pass `entry_source` and `entry_path` for the primary file being compiled
    /// so single-file diagnostics anchor correctly. Errors from resolve, type
    /// check, and lower passes include embedded [`DiagnosticContext`] and render
    /// multi-module carets without extra caller work.
    pub fn compile_error(
        &self,
        err: &CompileError,
        entry_source: Option<&str>,
        entry_path: Option<&std::path::Path>,
    ) {
        let path_buf = entry_path.map(|p| p.display().to_string());
        let path_ref = path_buf.as_deref();
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
        let msg = err.format_with_modules_styled(
            entry_source,
            path_ref,
            modules,
            interner,
            self.style as &dyn DiagnosticStyle,
        );
        eprint_line(&msg);
    }

    /// Reports a [`BuildError`] from project or module graph loading.
    ///
    /// Used when compilation never reaches the diagnostic bag — for example
    /// missing `phoenix.toml`, cyclic path dependencies, or invalid module
    /// layout.
    pub fn build_error(&self, err: &BuildError) {
        eprint_line(&self.style.plain_error(&err.to_message()));
    }

    /// Reports a project configuration or resolution failure.
    ///
    /// Emits a plain error line without compiler carets. Callers typically map
    /// workflow errors from [`crate::workflow`] to this method and return
    /// [`crate::exit::CliExit::Usage`].
    pub fn project_error(&self, message: &str) {
        eprint_line(&self.style.plain_error(message));
    }

    /// Reports a bytecode verification failure after compile.
    ///
    /// Prefixes the message with `verify error:` so scripts can distinguish
    /// verify failures from compile diagnostics.
    pub fn verify_error(&self, message: &str) {
        eprint_line(&self.style.plain_error(&format!("verify error: {message}")));
    }

    /// Reports a VM runtime failure without source mapping.
    ///
    /// Prefer [`Reporter::runtime_vm_error`] when a loaded [`BytecodeModule`]
    /// and [`SourceContext`] are available so faults cite Phoenix source lines.
    pub fn runtime_error(&self, message: &str) {
        eprint_line(&self.style.plain_error(&format!("runtime error: {message}")));
    }

    /// Reports a VM runtime failure with PHX0 section 5 source mapping when available.
    ///
    /// Formats `err` through [`crate::vm_diag::format_vm_error`] so trap sites,
    /// stack traces, and actor faults show file/line carets when the module
    /// embeds debug info.
    pub fn runtime_vm_error(
        &self,
        module: &BytecodeModule,
        err: &VmError,
        ctx: &SourceContext<'_>,
    ) {
        let message = format_vm_error(module, err, ctx);
        eprint_line(&self.style.plain_error(&format!("runtime error: {message}")));
    }

    /// Reports a generic I/O failure (read, write, or filesystem).
    pub fn io_error(&self, message: &str) {
        eprint_line(&self.style.plain_error(&format!("I/O error: {message}")));
    }

    /// Reports a usage or workflow error (missing arguments, invalid flags).
    ///
    /// Unlike [`Reporter::compile_error`], the message is emitted as-is without
    /// an extra prefix so callers control the full text.
    pub fn usage_error(&self, message: &str) {
        eprint_line(&self.style.plain_error(message));
    }

    /// Reports a compile failure summary without rendering a full diagnostic bag.
    ///
    /// Used for aggregate messages — for example when lint deny policy blocks
    /// the build after warnings were already printed separately.
    pub fn compile_message(&self, message: &str) {
        eprint_line(&self.style.plain_error(message));
    }

    /// Reports a success status line (for example the output path after `phx build`).
    pub fn success(&self, message: &str) {
        eprint_line(&self.style.success(message));
    }

    /// Reports cargo-style progress when a module is loaded for checking.
    ///
    /// Emitted during `phx check` project mode as each logical module is
    /// type-checked.
    pub fn checking_module(&self, logical_module: &str, path: &Path) {
        eprint_line(&self.style.checking_module(logical_module, path));
    }

    /// Reports cargo-style summary after a successful check.
    pub fn check_finished(&self, module_count: usize, elapsed: Duration) {
        eprint_line(&self.style.finished_checking(module_count, elapsed));
    }

    /// Reports verbose pipeline status when `enabled` is true.
    ///
    /// No-op when verbose mode is off, so callers can call this unconditionally
    /// at pipeline stage boundaries.
    pub fn verbose(&self, enabled: bool, message: &str) {
        if enabled {
            eprint_line(message);
        }
    }
}

/// Builds an [`AnsiStyle`] from a [`crate::color::ColorChoice`].
///
/// Convenience for handlers that need the style before constructing a
/// [`Reporter`]. Equivalent to [`crate::color::diagnostic_style`].
#[must_use]
pub fn reporter(choice: crate::color::ColorChoice) -> AnsiStyle {
    crate::color::diagnostic_style(choice)
}

fn eprint_line(msg: &str) {
    // CLI is allowed to write user-facing diagnostics to stderr.
    eprintln!("{msg}");
}

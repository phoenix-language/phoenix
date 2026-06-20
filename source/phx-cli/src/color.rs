//! ANSI styling for CLI output.
//!
//! This module runs **alongside** the parse → workflow → exit pipeline: global
//! `--color` from [`crate::args::CliOptions`] is resolved here and applied when
//! rendering diagnostics, progress lines, and success messages to stderr.
//!
//! ```text
//! CliOptions.color
//!       │
//!       ▼
//! ColorChoice::should_color ──► AnsiStyle ──► DiagnosticStyle
//!       │                              │
//!       │                              ├──► crate::report::Reporter
//!       │                              └──► crate::commands (progress / success)
//! ```
//!
//! [`ColorChoice::Auto`] respects the [`NO_COLOR`](https://no-color.org/)
//! environment variable and whether stderr is a terminal. [`AnsiStyle`]
//! implements [`DiagnosticStyle`] from `phx-diagnostics` so compile errors,
//! notes, and help labels share one styling path across subcommands.
//!
//! ## Public types
//!
//! - [`ColorChoice`] — `auto`, `always`, or `never` color policy.
//! - [`AnsiStyle`] — ANSI-aware [`DiagnosticStyle`] implementation.
//!
//! ## Entry points
//!
//! - [`ColorChoice::parse`] — parse `--color` flag values.
//! - [`ColorChoice::should_color`] — resolve whether to emit escape codes.
//! - [`diagnostic_style`] — construct an [`AnsiStyle`] from a [`ColorChoice`].

use std::io::{IsTerminal, stderr};
use std::path::Path;
use std::time::Duration;

use phx_diagnostics::DiagnosticStyle;

/// When to emit ANSI escape codes on stderr.
///
/// Parsed from `--color auto|always|never` via [`ColorChoice::parse`] and stored
/// in [`crate::args::CliOptions`]. Passed to [`diagnostic_style`] in every
/// command handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorChoice {
    /// Use color when stderr is a terminal and `NO_COLOR` is unset.
    #[default]
    Auto,
    /// Always emit ANSI codes.
    Always,
    /// Never emit ANSI codes.
    Never,
}

impl ColorChoice {
    /// Parses `--color auto|always|never`.
    ///
    /// # Errors
    ///
    /// Returns an error message when `value` is not recognized.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            other => Err(format!(
                "invalid value '{other}' for --color (expected auto, always, or never)"
            )),
        }
    }

    /// Resolves whether colors should be enabled for this invocation.
    ///
    /// `Auto` enables color when `NO_COLOR` is unset and stderr is a terminal.
    #[must_use]
    pub fn should_color(self) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => std::env::var_os("NO_COLOR").is_none() && stderr().is_terminal(),
        }
    }
}

/// ANSI-colored diagnostic and progress output.
///
/// Implements [`DiagnosticStyle`] for compile diagnostics and provides
/// cargo-style progress helpers ([`AnsiStyle::checking_module`],
/// [`AnsiStyle::finished_checking`]). Construct via [`diagnostic_style`] or
/// [`AnsiStyle::new`].
#[derive(Debug)]
pub struct AnsiStyle {
    enabled: bool,
}

impl AnsiStyle {
    /// Creates a style from a [`ColorChoice`], resolving terminal detection.
    #[must_use]
    pub fn new(choice: ColorChoice) -> Self {
        Self {
            enabled: choice.should_color(),
        }
    }
}

impl AnsiStyle {
    fn wrap(&self, text: &str, code: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }

    fn progress_line(&self, verb: &str, detail: &str) -> String {
        if self.enabled {
            format!("    \x1b[1;32m{verb}\x1b[0m {detail}")
        } else {
            format!("    {verb} {detail}")
        }
    }
}

impl DiagnosticStyle for AnsiStyle {
    fn error_header(&self, code: phx_diagnostics::DiagnosticCode, message: &str) -> String {
        self.wrap(&format!("error[{code}]: {message}"), "1;38;5;203")
    }

    fn location_line(&self, path: &str, line: u32, col: u32) -> String {
        self.wrap(&format!("  --> {path}:{line}:{col}"), "1;38;5;12")
    }

    fn note_label(&self, text: &str) -> String {
        self.wrap(&format!("   = note: {text}"), "38;5;14")
    }

    fn help_label(&self, text: &str) -> String {
        self.wrap(&format!("   = help: {text}"), "38;5;14")
    }

    fn abort_footer(&self, count: usize) -> String {
        let noun = if count == 1 { "error" } else { "errors" };
        self.wrap(
            &format!("error: aborting due to {count} previous {noun}"),
            "1;38;5;203",
        )
    }

    fn plain_error(&self, message: &str) -> String {
        self.wrap(&format!("error: {message}"), "1;38;5;203")
    }

    fn success(&self, message: &str) -> String {
        self.wrap(message, "38;5;2")
    }
}

impl AnsiStyle {
    /// Cargo-style progress line for a module entering type-check.
    #[must_use]
    pub fn checking_module(&self, logical_module: &str, path: &Path) -> String {
        let display = phx_diagnostics::diagnostic_display_path(path);
        self.progress_line("Checking", &format!("{logical_module} ({display})"))
    }

    /// Cargo-style summary after a successful `phx check`.
    #[must_use]
    pub fn finished_checking(&self, module_count: usize, elapsed: Duration) -> String {
        let secs = elapsed.as_secs_f64();
        let noun = if module_count == 1 {
            "module"
        } else {
            "modules"
        };
        self.progress_line(
            "Finished",
            &format!("checking {module_count} {noun} in {secs:.2}s"),
        )
    }
}

/// Returns the active diagnostic style for a color choice.
///
/// Convenience used by command handlers to build a [`crate::report::Reporter`].
#[must_use]
pub fn diagnostic_style(choice: ColorChoice) -> AnsiStyle {
    AnsiStyle::new(choice)
}

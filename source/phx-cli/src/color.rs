//! ANSI styling for CLI output.

use std::io::{IsTerminal, stderr};

use phx_diagnostics::DiagnosticStyle;

/// When to emit ANSI escape codes.
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

    /// Resolves whether colors should be enabled.
    #[must_use]
    pub fn should_color(self) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => std::env::var_os("NO_COLOR").is_none() && stderr().is_terminal(),
        }
    }
}

/// ANSI-colored diagnostic output.
#[derive(Debug)]
pub struct AnsiStyle {
    enabled: bool,
}

impl AnsiStyle {
    /// Creates a style from a [`ColorChoice`].
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
}

impl DiagnosticStyle for AnsiStyle {
    fn error_header(&self, code: phx_diagnostics::DiagnosticCode, message: &str) -> String {
        self.wrap(&format!("error[{code}]: {message}"), "1;38;5;196")
    }

    fn location_line(&self, path: &str, line: u32, col: u32) -> String {
        self.wrap(&format!("  --> {path}:{line}:{col}"), "1;38;5;12")
    }

    fn note_label(&self, text: &str) -> String {
        self.wrap(&format!("   = note: {text}"), "38;5;14")
    }

    fn abort_footer(&self, count: usize) -> String {
        let noun = if count == 1 { "error" } else { "errors" };
        self.wrap(
            &format!("error: aborting due to {count} previous {noun}"),
            "1;38;5;196",
        )
    }

    fn plain_error(&self, message: &str) -> String {
        self.wrap(&format!("error: {message}"), "1;38;5;196")
    }

    fn success(&self, message: &str) -> String {
        self.wrap(message, "38;5;2")
    }
}

/// Returns the active diagnostic style for a color choice.
#[must_use]
pub fn diagnostic_style(choice: ColorChoice) -> AnsiStyle {
    AnsiStyle::new(choice)
}

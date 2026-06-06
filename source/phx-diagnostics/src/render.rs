//! Cargo-style diagnostic rendering with optional styling.

use crate::DiagnosticCode;
use crate::Span;

/// Colors and emphasis for diagnostic output.
pub trait DiagnosticStyle: Send + Sync + std::fmt::Debug {
    /// Prefix for an error header, e.g. `error[E2001]: message`.
    fn error_header(&self, code: DiagnosticCode, message: &str) -> String;

    /// Location arrow line, e.g. `  --> path:line:col`.
    fn location_line(&self, path: &str, line: u32, col: u32) -> String;

    /// Secondary note label, e.g. `= note: ...`.
    fn note_label(&self, text: &str) -> String;

    /// Summary footer when multiple errors were emitted.
    fn abort_footer(&self, count: usize) -> String;

    /// Plain severity prefix for non-diagnostic errors (I/O, verify, etc.).
    fn plain_error(&self, message: &str) -> String;

    /// Success status line.
    fn success(&self, message: &str) -> String;
}

/// Unstyled output for golden tests and `--color never`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlainStyle;

impl DiagnosticStyle for PlainStyle {
    fn error_header(&self, code: DiagnosticCode, message: &str) -> String {
        format!("error[{code}]: {message}")
    }

    fn location_line(&self, path: &str, line: u32, col: u32) -> String {
        format!("  --> {path}:{line}:{col}")
    }

    fn note_label(&self, text: &str) -> String {
        format!("   = note: {text}")
    }

    fn abort_footer(&self, count: usize) -> String {
        let noun = if count == 1 { "error" } else { "errors" };
        format!("error: aborting due to {count} previous {noun}")
    }

    fn plain_error(&self, message: &str) -> String {
        format!("error: {message}")
    }

    fn success(&self, message: &str) -> String {
        message.to_owned()
    }
}

/// File path and logical module context for a rendered span.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpanContext<'a> {
    /// Filesystem path shown in the location line (clickable in editors).
    pub file_path: Option<&'a str>,
    /// Logical module path (`myapp::utils`).
    pub logical_module: Option<&'a str>,
}

/// Renders a primary diagnostic with source snippet and optional module note.
#[must_use]
pub fn render_diagnostic(
    style: &dyn DiagnosticStyle,
    source: &str,
    span: Span,
    code: DiagnosticCode,
    message: &str,
    ctx: SpanContext<'_>,
) -> String {
    let (line, col) = line_col(source, span.start);
    let path = ctx.file_path.unwrap_or("<entry>");
    let mut out = style.error_header(code, message);
    out.push('\n');
    out.push_str(&style.location_line(path, line, col));
    out.push('\n');
    out.push_str(&render_snippet(source, span, line));
    if let Some(module) = ctx.logical_module {
        out.push('\n');
        out.push_str(&style.note_label(&format!("in module `{module}`")));
    }
    out
}

/// Renders primary + secondary note spans (move site, duplicate def, etc.).
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn render_diagnostic_with_note(
    style: &dyn DiagnosticStyle,
    source: &str,
    span: Span,
    code: DiagnosticCode,
    message: &str,
    ctx: SpanContext<'_>,
    note_span: Span,
    note_text: &str,
) -> String {
    let primary = render_diagnostic(style, source, span, code, message, ctx);
    let (note_line, note_col) = line_col(source, note_span.start);
    let path = ctx.file_path.unwrap_or("<entry>");
    let mut out = primary;
    out.push('\n');
    out.push_str(&style.note_label(note_text));
    out.push('\n');
    out.push_str(&style.location_line(path, note_line, note_col));
    out.push('\n');
    out.push_str(&render_snippet(source, note_span, note_line));
    out
}

/// Joins multiple rendered diagnostics with blank lines and an optional footer.
#[must_use]
pub fn join_diagnostics(style: &dyn DiagnosticStyle, parts: &[String]) -> String {
    if parts.is_empty() {
        return String::new();
    }
    let mut out = parts.join("\n\n");
    if parts.len() > 1 {
        out.push('\n');
        out.push_str(&style.abort_footer(parts.len()));
    }
    out
}

fn render_snippet(source: &str, span: Span, _line_hint: u32) -> String {
    let (line, col) = line_col(source, span.start);
    let line_idx = usize::try_from(line.saturating_sub(1)).unwrap_or(0);
    let line_text = source.lines().nth(line_idx).unwrap_or("");
    let caret_len = if span.end > span.start {
        span.end.saturating_sub(span.start)
    } else {
        1
    };
    let caret = "^".repeat(usize::try_from(caret_len.min(40)).unwrap_or(1));
    let pad = " ".repeat(usize::try_from(col.saturating_sub(1)).unwrap_or(0));
    let gutter = line.to_string();
    let gutter_width = gutter.len().max(1);
    format!("   |\n{gutter:>gutter_width$} | {line_text}\n   | {pad}{caret}")
}

/// Returns 1-based `(line, column)` for a byte offset (columns count Unicode scalars).
#[must_use]
pub fn line_col(source: &str, byte: u32) -> (u32, u32) {
    let byte = usize::try_from(byte).unwrap_or(0);
    let mut line = 1u32;
    let mut col = 1u32;
    for (i, ch) in source.char_indices() {
        if i >= byte {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Short explanation for `phx explain E####`.
#[must_use]
pub fn explain_code(code: &str) -> Option<&'static str> {
    match code {
        "E1001" => Some("An identifier could not be resolved in the current scope."),
        "E1008" => Some("A binary package must define `main :: () => { ... }` in the root module."),
        "E2001" => Some("An expression's type does not match the expected type."),
        "E2017" => Some("A value was used after it was moved."),
        "E3001" => Some("The parser encountered unexpected tokens."),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_style_contains_path_and_gutter() {
        let src = "main :: () => {\n    const x: s32 = true;\n};";
        let span = Span::new(28, 32);
        let style = PlainStyle;
        let out = render_diagnostic(
            &style,
            src,
            span,
            DiagnosticCode::new("E2001"),
            "type mismatch",
            SpanContext {
                file_path: Some("bad_type.phx"),
                logical_module: Some("app"),
            },
        );
        assert!(out.contains("error[E2001]:"));
        assert!(out.contains("--> bad_type.phx:"));
        assert!(out.contains("in module `app`"));
        assert!(out.contains(" | "));
    }
}

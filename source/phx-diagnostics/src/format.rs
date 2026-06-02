//! Source span rendering with carets for CLI diagnostics.

use crate::Span;

/// Formats `message` with a source line and caret for `span` in `source`.
#[must_use]
pub fn format_span_message(source: &str, span: Span, message: &str) -> String {
    let (line, col) = line_col(source, span.start);
    let line_text = source
        .lines()
        .nth(usize::try_from(line.saturating_sub(1)).unwrap_or(0))
        .unwrap_or("");
    let caret_len = if span.end > span.start {
        span.end.saturating_sub(span.start)
    } else {
        1
    };
    let caret = "^".repeat(usize::try_from(caret_len.min(40)).unwrap_or(1));
    format!(
        "error: {message}\n --> line {line}, column {col}\n  |\n  | {line_text}\n  | {}{caret}",
        " ".repeat(usize::try_from(col.saturating_sub(1)).unwrap_or(0)),
    )
}

fn line_col(source: &str, byte: u32) -> (u32, u32) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_on_second_line() {
        let src = "main :: () => {\n    x;\n};";
        let span = Span::new(17, 18);
        let out = format_span_message(src, span, "type mismatch");
        assert!(out.contains("line 2"));
        assert!(out.contains('^'));
    }
}

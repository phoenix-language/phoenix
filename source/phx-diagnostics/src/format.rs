//! Source span rendering with carets for CLI diagnostics.

use crate::Span;
use crate::TypeCheckError;

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

/// Formats a type-check error with source carets; includes a secondary note for move errors.
#[must_use]
pub fn format_typecheck_error(source: &str, err: &TypeCheckError) -> String {
    match err {
        TypeCheckError::UseAfterMove {
            name,
            move_span,
            span,
            ..
        } => format_span_message_with_note(
            source,
            *span,
            &format!("use of moved value `{name}`"),
            *move_span,
            "value moved here",
        ),
        other => {
            if let Some(span) = other.span() {
                format_span_message(source, span, &other.to_string())
            } else {
                other.to_string()
            }
        }
    }
}

/// Formats `message` at `span` plus an optional `note_label` at `note_span`.
#[must_use]
pub fn format_span_message_with_note(
    source: &str,
    span: Span,
    message: &str,
    note_span: Span,
    note_label: &str,
) -> String {
    let primary = format_span_message(source, span, message);
    let note = format_span_message(source, note_span, "");
    let note_body = note.lines().skip(1).collect::<Vec<_>>().join("\n");
    format!("{primary}\nnote: {note_label}\n{note_body}")
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

    #[test]
    fn use_after_move_note() {
        let src = "main :: () => {\n    var p = q;\n    const _ = p;\n};";
        let err = TypeCheckError::UseAfterMove {
            name: "p".to_owned(),
            move_span: Span::new(22, 23),
            span: Span::new(38, 39),
        };
        let out = format_typecheck_error(src, &err);
        assert!(out.contains("use of moved value `p`"));
        assert!(out.contains("value moved here"));
        assert!(out.contains("note:"));
    }
}

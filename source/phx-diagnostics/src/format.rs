//! Source span rendering with carets for CLI diagnostics.

use crate::LexError;
use crate::SymbolNames;
use crate::ResolveError;
use crate::Span;
use crate::TypeCheckError;
use crate::code::DiagnosticCode;

fn append_code(message: String, code: DiagnosticCode) -> String {
    format!("{message} [{code}]")
}

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

/// Formats a lexical error with a source caret when possible.
#[must_use]
pub fn format_lex_error(source: &str, err: &LexError) -> String {
    let message = lex_message(err);
    let code = err.code();
    if let Some(span) = err.span() {
        append_code(format_span_message(source, span, &message), code)
    } else {
        append_code(message, code)
    }
}

fn lex_message(err: &LexError) -> String {
    match err {
        LexError::UnterminatedString { .. } => {
            "unterminated byte string or character literal".to_owned()
        }
        LexError::UnterminatedBlockComment { .. } => "unterminated block comment".to_owned(),
        LexError::InvalidEscape { .. } => "invalid escape sequence".to_owned(),
        LexError::UnexpectedChar { ch, .. } => format!("unexpected character {ch:?}"),
        LexError::IntegerOverflow { .. } => "integer literal overflow".to_owned(),
        LexError::InvalidInt { .. } => "invalid integer literal".to_owned(),
        LexError::InvalidFloat { .. } => "invalid float literal".to_owned(),
        LexError::LexemeTooLong { .. } => "lexeme too long".to_owned(),
    }
}

/// Human-readable message for a resolve error (no caret).
#[must_use]
pub fn resolve_message(names: &impl SymbolNames, err: &ResolveError) -> String {
    match err {
        ResolveError::UnresolvedIdent { symbol_index, .. } => {
            let name = names.symbol_name(*symbol_index);
            format!("unresolved identifier `{name}`")
        }
        ResolveError::UnresolvedType { symbol_index, .. } => {
            let name = names.symbol_name(*symbol_index);
            format!("unresolved type `{name}`")
        }
        ResolveError::DuplicateDefinition { symbol_index, .. } => {
            let name = names.symbol_name(*symbol_index);
            format!("duplicate definition of `{name}`")
        }
        other => other.to_string(),
    }
}

/// Formats a resolve error with source carets and interned names.
#[must_use]
pub fn format_resolve_error(source: &str, names: &impl SymbolNames, err: &ResolveError) -> String {
    let code = err.code();
    let message = resolve_message(names, err);
    let body = match err {
        ResolveError::DuplicateDefinition {
            first_span,
            span,
            symbol_index,
            ..
        } => {
            let name = names.symbol_name(*symbol_index);
            format_span_message_with_note(
                source,
                *span,
                &format!("duplicate definition of `{name}`"),
                *first_span,
                "previous definition here",
            )
        }
        other => {
            if let Some(span) = other.span() {
                format_span_message(source, span, &message)
            } else {
                message
            }
        }
    };
    append_code(body, code)
}

/// Human-readable message for a type-check error (no caret).
#[must_use]
pub fn typecheck_message(names: &impl SymbolNames, err: &TypeCheckError) -> String {
    match err {
        TypeCheckError::UnknownType { symbol_index, .. } => {
            let name = names.symbol_name(*symbol_index);
            format!("unknown type `{name}`")
        }
        TypeCheckError::UnresolvedValue { symbol_index, .. } => {
            let name = names.symbol_name(*symbol_index);
            format!("unresolved value `{name}`")
        }
        TypeCheckError::UnresolvedMethod {
            receiver,
            method_index,
            ..
        } => {
            let name = names.symbol_name(*method_index);
            format!("no method `{name}` on type `{receiver}`")
        }
        TypeCheckError::AmbiguousMethod {
            receiver,
            method_index,
            ..
        } => {
            let name = names.symbol_name(*method_index);
            format!(
                "ambiguous method `{name}` on type `{receiver}` (multiple trait impls)"
            )
        }
        other => other.to_string(),
    }
}

/// Formats a type-check error with source carets; includes secondary notes for move errors.
#[must_use]
pub fn format_typecheck_error(source: &str, names: &impl SymbolNames, err: &TypeCheckError) -> String {
    let code = err.code();
    let body = match err {
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
        TypeCheckError::MovedAssignTarget {
            name,
            move_span,
            span,
            ..
        } => format_span_message_with_note(
            source,
            *span,
            &format!("cannot assign to moved value `{name}`"),
            *move_span,
            "value moved here",
        ),
        other => {
            let message = typecheck_message(names, other);
            if let Some(span) = other.span() {
                format_span_message(source, span, &message)
            } else {
                message
            }
        }
    };
    append_code(body, code)
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
    use crate::ResolveError;

    struct TestNames;

    impl SymbolNames for TestNames {
        fn symbol_name(&self, symbol_index: u32) -> &str {
            if symbol_index == 0 {
                "foo"
            } else {
                "?"
            }
        }
    }

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
        let names = TestNames;
        let err = TypeCheckError::UseAfterMove {
            name: "p".to_owned(),
            move_span: Span::new(22, 23),
            span: Span::new(38, 39),
        };
        let out = format_typecheck_error(src, &names, &err);
        assert!(out.contains("use of moved value `p`"));
        assert!(out.contains("value moved here"));
        assert!(out.contains("note:"));
    }

    #[test]
    fn duplicate_definition_note() {
        let src = "foo :: () => { };\nfoo :: () => { };";
        let names = TestNames;
        let err = ResolveError::DuplicateDefinition {
            symbol_index: 0,
            first_span: Span::new(0, 3),
            span: Span::new(18, 21),
        };
        let out = format_resolve_error(src, &names, &err);
        assert!(out.contains("duplicate definition"));
        assert!(out.contains("previous definition here"));
        assert!(out.contains("note:"));
    }
}

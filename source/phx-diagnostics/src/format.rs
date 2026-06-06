//! Source span rendering with carets for CLI diagnostics.

use crate::LexError;
use crate::LowerError;
use crate::ResolveError;
use crate::Span;
use crate::SymbolNames;
use crate::TypeCheckError;
use crate::code::DiagnosticCode;
use crate::render::{
    AncillaryNote, DiagnosticAncillary, PlainStyle, SpanContext, render_diagnostic,
    render_diagnostic_enriched, render_diagnostic_with_note,
};
use crate::type_notes::{TypeCheckNote, typecheck_ancillary};

/// Formats `message` with a source line and caret for `span` in `source`.
#[must_use]
pub fn format_span_message(source: &str, span: Span, message: &str) -> String {
    let style = PlainStyle;
    render_diagnostic(
        &style,
        source,
        span,
        DiagnosticCode::new("E0000"),
        message,
        SpanContext::default(),
    )
}

/// Formats a lexical error with a source caret when possible.
#[must_use]
pub fn format_lex_error(source: &str, err: &LexError) -> String {
    format_lex_error_styled(source, err, &PlainStyle, SpanContext::default())
}

/// Formats a lexical error with styling and file context.
#[must_use]
pub fn format_lex_error_styled(
    source: &str,
    err: &LexError,
    style: &dyn crate::render::DiagnosticStyle,
    ctx: SpanContext<'_>,
) -> String {
    let message = lex_message(err);
    let code = err.code();
    if let Some(span) = err.span() {
        render_diagnostic(style, source, span, code, &message, ctx)
    } else {
        style.error_header(code, &message)
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
        LexError::InvalidUtf8 { .. } => "invalid UTF-8 in string literal".to_owned(),
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
        ResolveError::ModuleParse { message, .. } => message.clone(),
        ResolveError::ModuleIo { message, .. } => format!("failed to read module: {message}"),
        other => other.to_string(),
    }
}

/// Formats a resolve error with source carets and interned names.
#[must_use]
pub fn format_resolve_error(source: &str, names: &impl SymbolNames, err: &ResolveError) -> String {
    format_resolve_error_styled(source, names, err, &PlainStyle, SpanContext::default())
}

/// Formats a resolve error with styling and file context.
#[must_use]
pub fn format_resolve_error_styled(
    source: &str,
    names: &impl SymbolNames,
    err: &ResolveError,
    style: &dyn crate::render::DiagnosticStyle,
    ctx: SpanContext<'_>,
) -> String {
    let code = err.code();
    let message = resolve_message(names, err);
    let ctx = match err {
        ResolveError::ModuleParse { path, .. } | ResolveError::ModuleIo { path, .. } => {
            SpanContext {
                file_path: Some(path.as_str()),
                logical_module: ctx.logical_module,
            }
        }
        _ => ctx,
    };
    match err {
        ResolveError::DuplicateDefinition {
            first_span,
            span,
            symbol_index,
            ..
        } => {
            let name = names.symbol_name(*symbol_index);
            render_diagnostic_with_note(
                style,
                source,
                *span,
                code,
                &format!("duplicate definition of `{name}`"),
                ctx,
                *first_span,
                "previous definition here",
            )
        }
        other => {
            if let Some(span) = other.span() {
                render_diagnostic(style, source, span, code, &message, ctx)
            } else {
                style.error_header(code, &message)
            }
        }
    }
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
            format!("ambiguous method `{name}` on type `{receiver}` (multiple trait impls)")
        }
        other => other.to_string(),
    }
}

/// Formats a type-check error with source carets; includes secondary notes for move errors.
#[must_use]
pub fn format_typecheck_error(
    source: &str,
    names: &impl SymbolNames,
    err: &TypeCheckError,
) -> String {
    format_typecheck_error_styled(source, names, err, &PlainStyle, SpanContext::default())
}

/// Formats a type-check error with styling and file context.
#[must_use]
pub fn format_typecheck_error_styled(
    source: &str,
    names: &impl SymbolNames,
    err: &TypeCheckError,
    style: &dyn crate::render::DiagnosticStyle,
    ctx: SpanContext<'_>,
) -> String {
    let code = err.code();
    let message = typecheck_message(names, err);
    let ancillary = typecheck_ancillary(names, err);
    if let Some(span) = err.span() {
        render_typecheck_diagnostic(style, source, span, code, &message, ctx, &ancillary)
    } else {
        let mut out = style.error_header(code, &message);
        append_ancillary_text(style, &mut out, &ancillary);
        out
    }
}

fn render_typecheck_diagnostic(
    style: &dyn crate::render::DiagnosticStyle,
    source: &str,
    span: Span,
    code: DiagnosticCode,
    message: &str,
    ctx: SpanContext<'_>,
    ancillary: &crate::type_notes::TypeCheckAncillary,
) -> String {
    let (notes, helps) = ancillary_slices(ancillary);
    render_diagnostic_enriched(
        style,
        source,
        span,
        code,
        message,
        ctx,
        &DiagnosticAncillary {
            notes: &notes,
            helps: &helps,
        },
    )
}

fn ancillary_slices(
    ancillary: &crate::type_notes::TypeCheckAncillary,
) -> (Vec<AncillaryNote<'_>>, Vec<String>) {
    let notes: Vec<AncillaryNote<'_>> = ancillary
        .notes
        .iter()
        .map(|TypeCheckNote { text, span }| AncillaryNote {
            text: text.as_str(),
            span: *span,
        })
        .collect();
    (notes, ancillary.helps.clone())
}

fn append_ancillary_text(
    style: &dyn crate::render::DiagnosticStyle,
    out: &mut String,
    ancillary: &crate::type_notes::TypeCheckAncillary,
) {
    for note in &ancillary.notes {
        out.push('\n');
        out.push_str(&style.note_label(&note.text));
    }
    for help in &ancillary.helps {
        out.push('\n');
        out.push_str(&style.help_label(help));
    }
}

/// Formats a lowering error with a source caret when possible.
#[must_use]
pub fn format_lower_error(source: &str, err: &LowerError) -> String {
    format_lower_error_styled(source, err, &PlainStyle, SpanContext::default())
}

/// Formats a lowering error with styling and file context.
#[must_use]
pub fn format_lower_error_styled(
    source: &str,
    err: &LowerError,
    style: &dyn crate::render::DiagnosticStyle,
    ctx: SpanContext<'_>,
) -> String {
    let code = err.code();
    let message = err.to_string();
    if let Some(span) = err.span() {
        render_diagnostic(style, source, span, code, &message, ctx)
    } else {
        style.error_header(code, &message)
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
    let style = PlainStyle;
    render_diagnostic_with_note(
        &style,
        source,
        span,
        DiagnosticCode::new("E0000"),
        message,
        SpanContext::default(),
        note_span,
        note_label,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResolveError;

    struct TestNames;

    impl SymbolNames for TestNames {
        fn symbol_name(&self, symbol_index: u32) -> &str {
            if symbol_index == 0 { "foo" } else { "?" }
        }
    }

    #[test]
    fn caret_on_second_line() {
        let src = "main :: () => {\n    x;\n};";
        let span = Span::new(17, 18);
        let out = format_span_message(src, span, "type mismatch");
        assert!(out.contains(":2:"));
        assert!(out.contains('^'));
    }

    #[test]
    fn const_binding_mismatch_note_and_help() {
        let src = "main :: () => { const x: s32 = true; };";
        let names = TestNames;
        let err = TypeCheckError::Mismatch {
            expected: "S32".to_owned(),
            found: "Bool".to_owned(),
            span: Span::new(28, 32),
            kind: crate::MismatchKind::ConstBinding {
                name: "x".to_owned(),
                annotation_span: Span::new(22, 25),
            },
        };
        let out = format_typecheck_error(src, &names, &err);
        assert!(out.contains("expected type `S32` due to type annotation on `const x`"));
        assert!(out.contains("= help:"));
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
        assert!(out.contains("value `p` was moved here"));
        assert!(out.contains("= note:"));
        assert!(out.contains("= help:"));
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
        assert!(out.contains("= note:"));
    }
}

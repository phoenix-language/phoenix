//! Source span rendering with carets for CLI diagnostics.

use crate::IrError;
use crate::LexError;
use crate::LowerError;
use crate::ParseBag;
use crate::ParseError;
use crate::ResolveError;
use crate::Span;
use crate::SymbolNames;
use crate::TypeCheckError;
use crate::code::DiagnosticCode;
use crate::render::{
    AncillaryNote, DiagnosticAncillary, PlainStyle, SpanContext, join_diagnostics,
    render_diagnostic, render_diagnostic_enriched, render_diagnostic_with_note,
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

/// Human-readable message for a parse error (no caret).
#[must_use]
pub fn parse_message(err: &ParseError) -> String {
    match err {
        ParseError::Lex(e) => lex_message(e),
        ParseError::UnexpectedToken {
            expected, found, ..
        } => format!("expected {expected}, found {found}"),
        ParseError::UnexpectedEof { expected, .. } => {
            format!("expected {expected}, found end of file")
        }
        ParseError::UnsupportedSyntax { feature, .. } => {
            format!("unsupported syntax: {feature}")
        }
        ParseError::InvalidPattern { .. } => "invalid pattern".to_owned(),
        ParseError::InternTableFull { .. } => "identifier intern table is full".to_owned(),
    }
}

/// Formats a parse error with a source caret when possible.
#[must_use]
pub fn format_parse_error(source: &str, err: &ParseError) -> String {
    format_parse_error_styled(Some(source), err, &PlainStyle, SpanContext::default())
}

/// Formats a parse error with styling and file context.
#[must_use]
pub fn format_parse_error_styled(
    source: Option<&str>,
    err: &ParseError,
    style: &dyn crate::render::DiagnosticStyle,
    ctx: SpanContext<'_>,
) -> String {
    match err {
        ParseError::Lex(e) => match source {
            Some(src) => format_lex_error_styled(src, e, style, ctx),
            None => style.error_header(e.code(), &lex_message(e)),
        },
        other => {
            let code = other.code();
            let message = parse_message(other);
            match (source, other.span()) {
                (Some(src), Some(span)) => render_diagnostic(style, src, span, code, &message, ctx),
                _ => style.error_header(code, &message),
            }
        }
    }
}

/// Formats all errors in a parse bag, joined for multi-error output.
#[must_use]
pub fn format_parse_bag_styled(
    bag: &ParseBag,
    source: Option<&str>,
    ctx: SpanContext<'_>,
    style: &dyn crate::render::DiagnosticStyle,
) -> String {
    let parts: Vec<String> = bag
        .errors()
        .iter()
        .map(|err| format_parse_error_styled(source, err, style, ctx))
        .collect();
    join_diagnostics(style, &parts)
}

/// Human-readable message for a resolve error (no caret).
#[must_use]
pub fn resolve_message(names: &impl SymbolNames, err: &ResolveError) -> String {
    match err {
        ResolveError::UnresolvedIdent { symbol_index, .. } => {
            let name = names.symbol_name(*symbol_index).unwrap_or("<?>");
            format!("unresolved identifier `{name}`")
        }
        ResolveError::UnresolvedType { symbol_index, .. } => {
            let name = names.symbol_name(*symbol_index).unwrap_or("<?>");
            format!("unresolved type `{name}`")
        }
        ResolveError::DuplicateDefinition { symbol_index, .. } => {
            let name = names.symbol_name(*symbol_index).unwrap_or("<?>");
            format!("duplicate definition of `{name}`")
        }
        ResolveError::ModuleParse { message, .. } => message.clone(),
        ResolveError::ModuleIo { message, .. } => format!("failed to read module: {message}"),
        ResolveError::ProgramTooLarge { .. } => {
            "program too large (definition table exceeds limit)".to_owned()
        }
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
            let name = names.symbol_name(*symbol_index).unwrap_or("<?>");
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
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn typecheck_message(names: &impl SymbolNames, err: &TypeCheckError) -> String {
    match err {
        TypeCheckError::Mismatch {
            expected, found, ..
        } => format!("type mismatch: expected {expected}, found {found}"),
        TypeCheckError::UnknownType { symbol_index, .. } => {
            format!("unknown type ({})", sym_label(names, *symbol_index))
        }
        TypeCheckError::ArityMismatch {
            expected, found, ..
        } => format!("argument count mismatch: expected {expected}, found {found}"),
        TypeCheckError::NotCallable { found, .. } => {
            format!("value of type `{found}` is not callable")
        }
        TypeCheckError::UnresolvedMethod {
            receiver,
            method_index,
            ..
        } => format!(
            "no method {} on type `{receiver}`",
            sym_label(names, *method_index)
        ),
        TypeCheckError::AmbiguousMethod {
            receiver,
            method_index,
            ..
        } => format!(
            "ambiguous method {} on type `{receiver}` (multiple trait impls)",
            sym_label(names, *method_index)
        ),
        TypeCheckError::NonUnifyingBranches { .. } => "branch types do not unify".to_owned(),
        TypeCheckError::NonExhaustiveMatch { missing, .. } => {
            if missing.is_empty() {
                "non-exhaustive `match` on enum".to_owned()
            } else {
                format!(
                    "non-exhaustive `match`: missing variant(s) {}",
                    missing.join(", ")
                )
            }
        }
        TypeCheckError::UnreachableMatchArm { reason, .. } => {
            format!("unreachable `match` arm: {reason}")
        }
        TypeCheckError::UnknownStructField { name, .. } => {
            format!("struct literal has no field `{name}`")
        }
        TypeCheckError::MissingStructField { name, .. } => {
            format!("struct literal is missing field `{name}`")
        }
        TypeCheckError::UnknownEnumVariantField { name, .. } => {
            format!("enum variant literal has no field `{name}`")
        }
        TypeCheckError::MissingEnumVariantField { name, .. } => {
            format!("enum variant literal is missing field `{name}`")
        }
        TypeCheckError::InvalidCast { from, to, .. } => {
            format!("invalid cast from `{from}` to `{to}`")
        }
        TypeCheckError::InvalidOperator { op, .. } => {
            format!("invalid use of operator `{op}`")
        }
        TypeCheckError::UnsupportedFeature { feature, .. } => {
            format!("{feature} is not available in MVP")
        }
        TypeCheckError::UseAfterMove { name, .. } => format!("use of moved value `{name}`"),
        TypeCheckError::MovedAssignTarget { name, .. } => {
            format!("cannot assign to moved value `{name}`")
        }
        TypeCheckError::UnresolvedValue { symbol_index, .. } => {
            format!("unresolved value ({})", sym_label(names, *symbol_index))
        }
        TypeCheckError::LoopControlOutsideLoop { keyword, .. } => {
            format!("`{keyword}` outside of a loop")
        }
        TypeCheckError::RecursiveTypeAlias { .. } => "recursive type alias".to_owned(),
        TypeCheckError::ReturnEscapesLocal { .. } => {
            "cannot return a borrow of a local variable".to_owned()
        }
        TypeCheckError::TraitNotSatisfied {
            type_name,
            trait_name,
            ..
        } => format!("type `{type_name}` does not satisfy trait bound `{trait_name}`"),
        TypeCheckError::UnknownTraitBound { trait_name, .. } => {
            format!("unknown trait bound `{trait_name}`")
        }
        TypeCheckError::InferenceFailed { .. } => {
            "could not infer generic type arguments from call arguments".to_owned()
        }
        TypeCheckError::InferenceAmbiguous { .. } => {
            "ambiguous generic type argument inference".to_owned()
        }
        TypeCheckError::MissingTraitMethod {
            type_name,
            trait_name,
            method_name,
            ..
        } => format!(
            "type `{type_name}` does not implement trait method `{method_name}` from `{trait_name}`"
        ),
        TypeCheckError::MissingAssociatedType {
            type_name,
            trait_name,
            assoc_name,
            ..
        } => format!(
            "type `{type_name}` does not specify associated type `{assoc_name}` from `{trait_name}`"
        ),
        TypeCheckError::TryOutsideFunction { .. } => {
            "`?` is only valid inside a function returning `Option` or `Result`".to_owned()
        }
        TypeCheckError::InvalidTryOperand {
            found,
            expected_return,
            ..
        } => format!("cannot apply `?` to `{found}` in function returning `{expected_return}`"),
        TypeCheckError::TryErrorFromMissing {
            err_in, err_out, ..
        } => format!(
            "cannot use `?` on `Result<_, {err_in}>` in function returning `Result<_, {err_out}`: no `From<{err_in}>` implementation for `{err_out}`"
        ),
        TypeCheckError::ExternCallRequiresUnsafe { name, .. } => {
            format!("call to foreign function `{name}` requires `unsafe`")
        }
        TypeCheckError::IntrinsicRequiresUnsafe { name, .. } => {
            format!("call to intrinsic `{name}` requires `unsafe`")
        }
        TypeCheckError::UnsafeFnCallRequiresUnsafe { name, .. } => {
            format!("call to `{name}` requires `unsafe`")
        }
        TypeCheckError::UnsafeTraitRequiresUnsafeImpl { trait_name, .. } => {
            format!("implementation of `unsafe trait` `{trait_name}` must use `unsafe impl`")
        }
        TypeCheckError::RedundantUnsafeInUnsafeTrait { method, .. } => format!(
            "redundant `unsafe` on method `{method}` in `unsafe trait` (methods inherit unsafety)"
        ),
        TypeCheckError::UnsafeImplOfSafeTrait { type_name, .. } => {
            format!("`unsafe impl` of `{type_name}` is only allowed for an `unsafe trait`")
        }
        TypeCheckError::CopyableDropConflict { type_name, .. } => {
            format!("type `{type_name}` cannot implement both `Drop` and `Copyable`")
        }
        TypeCheckError::InternalError { detail, .. } => {
            format!("internal error: {detail}")
        }
        TypeCheckError::ProgramTooLarge { .. } => {
            "program too large (definition table exceeds limit)".to_owned()
        }
        TypeCheckError::DiscardedStdResult { .. } => {
            "discarded `Result` value must be handled".to_owned()
        }
        TypeCheckError::DiscardedStdOption { .. } => {
            "discarded `Option` value must be handled".to_owned()
        }
    }
}

fn sym_label(names: &impl SymbolNames, symbol_index: u32) -> String {
    names
        .symbol_name(symbol_index)
        .map_or_else(|| format!("sym#{symbol_index}"), |name| format!("`{name}`"))
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

/// Formats an IR validation error with a source caret when possible.
#[must_use]
pub fn format_ir_error(source: &str, err: &IrError) -> String {
    format_ir_error_styled(source, err, &PlainStyle, SpanContext::default())
}

/// Formats an IR validation error with styling and file context.
#[must_use]
pub fn format_ir_error_styled(
    source: &str,
    err: &IrError,
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
    use crate::ExpectedToken;
    use crate::LexError;
    use crate::ResolveError;
    use std::borrow::Cow;

    struct TestNames;

    impl SymbolNames for TestNames {
        fn symbol_name(&self, symbol_index: u32) -> Option<&str> {
            if symbol_index == 0 {
                Some("foo")
            } else {
                Some("?")
            }
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

    #[test]
    fn unexpected_token_with_caret() {
        let src = "main :: () => { x + ; };";
        let err = ParseError::UnexpectedToken {
            expected: ExpectedToken::Expr,
            found: Cow::Borrowed("';'"),
            span: Span::new(20, 21),
        };
        let out = format_parse_error(src, &err);
        assert!(out.contains("expected expression, found ';'"));
        assert!(out.contains('^'));
    }

    #[test]
    fn unexpected_eof_message() {
        let err = ParseError::UnexpectedEof {
            expected: ExpectedToken::Punct("}"),
            span: Span::new(10, 10),
        };
        assert_eq!(parse_message(&err), "expected }, found end of file");
    }

    #[test]
    fn parse_lex_delegates_to_lex_formatter() {
        let src = "main :: () => { \"unclosed };";
        let lex_err = LexError::UnterminatedString { start: 18 };
        let parse_err = ParseError::Lex(lex_err);
        let from_parse = format_parse_error(src, &parse_err);
        let from_lex = format_lex_error(src, &LexError::UnterminatedString { start: 18 });
        assert_eq!(from_parse, from_lex);
        assert!(!from_parse.contains("lex error:"));
    }
}

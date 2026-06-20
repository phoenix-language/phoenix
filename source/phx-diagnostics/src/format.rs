//! Error-type → rendered diagnostic adapter for the Phoenix compiler.
//!
//! This module sits between structured pass errors ([`LexError`], [`ParseError`],
//! [`ResolveError`], [`TypeCheckError`], [`LowerError`], [`IrError`]) and the layout engine
//! in [`crate::render`]. Each `format_*` function:
//!
//! 1. Maps a variant to a stable [`DiagnosticCode`] and human-readable message (via `*_message`
//!    helpers when only the text is needed).
//! 2. Optionally attaches secondary notes (move sites, duplicate-definition locations, module
//!    paths) through [`crate::type_notes::typecheck_ancillary`] or hard-coded note labels.
//! 3. Delegates snippet layout (header, `--> path:line:col`, caret underline) to
//!    [`render_diagnostic`], [`render_diagnostic_enriched`], or [`render_diagnostic_with_note`].
//!
//! ## Division of responsibility
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | `format` (this file) | Pass-specific message text, error-code selection, ancillary notes |
//! | [`crate::render`] | Source-line extraction, caret alignment, styling hooks, lint layout |
//!
//! Prefer plain `format_*` entry points (they use [`PlainStyle`] and default [`SpanContext`])
//! for golden tests and `--color never`. Use `*_styled` variants in the CLI when a custom
//! [`DiagnosticStyle`] or file path context is available.
//!
//! ## Entry points by pass
//!
//! - **Lex** — [`format_lex_error`], [`format_lex_error_styled`]
//! - **Parse** — [`parse_message`], [`format_parse_error`], [`format_parse_bag_styled`],
//!   [`format_parse_bag_messages`], [`prepend_parse_bag_styled`]
//! - **Resolve** — [`resolve_message`], [`format_resolve_error`], [`format_resolve_error_styled`]
//! - **Type check** — [`typecheck_message`], [`format_typecheck_error`],
//!   [`format_typecheck_error_styled`]
//! - **Lower / IR** — [`format_lower_error`], [`format_ir_error`] and their `*_styled` variants
//! - **Ad hoc** — [`format_span_message`], [`format_span_message_with_note`] for tests and
//!   internal callers that already have a message string

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

/// Renders `message` at `span` using the generic placeholder code `E0000`.
///
/// Convenience wrapper around [`render_diagnostic`] for tests and ad hoc diagnostics that do
/// not belong to a specific pass error enum. Uses [`PlainStyle`] and default [`SpanContext`].
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans; invalid offsets produce a
/// degraded snippet without a caret.
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

/// Formats a lexical error as a full Cargo-style diagnostic.
///
/// When [`LexError::span`] is present, includes a source snippet and caret; otherwise returns
/// only the error header (code + message). Delegates to [`format_lex_error_styled`] with
/// [`PlainStyle`].
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
#[must_use]
pub fn format_lex_error(source: &str, err: &LexError) -> String {
    format_lex_error_styled(source, err, &PlainStyle, SpanContext::default())
}

/// Formats a lexical error with a caller-supplied style and file context.
///
/// Maps each [`LexError`] variant to a stable message string and [`LexError::code`]. When the
/// error carries a span, renders via [`render_diagnostic`]; otherwise uses
/// [`DiagnosticStyle::error_header`] alone.
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
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

/// Returns the human-readable message for a parse error without codes or source snippets.
///
/// Used by the CLI for terse output and by [`format_parse_bag_messages`]. Delegates to
/// [`lex_message`] for [`ParseError::Lex`] variants.
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

/// Formats a parse error as a full Cargo-style diagnostic.
///
/// When `source` is available and the error (or nested lex error) has a span, includes a source
/// snippet and caret. Uses [`PlainStyle`] and default [`SpanContext`].
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
#[must_use]
pub fn format_parse_error(source: &str, err: &ParseError) -> String {
    format_parse_error_styled(Some(source), err, &PlainStyle, SpanContext::default())
}

/// Formats a parse error with a caller-supplied style and optional source buffer.
///
/// [`ParseError::Lex`] delegates to [`format_lex_error_styled`] when `source` is `Some`; other
/// variants use [`parse_message`] and [`render_diagnostic`] when both source and span are
/// available.
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
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

/// Formats every error in a [`ParseBag`] and joins them for multi-error output.
///
/// Each error is rendered via [`format_parse_error_styled`]; parts are separated with blank
/// lines through [`join_diagnostics`].
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
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

/// Joins parse error messages without codes, carets, or file paths.
///
/// Messages are separated by `\n---\n`. Suitable for compact logging or non-snippet UIs.
#[must_use]
pub fn format_parse_bag_messages(bag: &ParseBag) -> String {
    let parts: Vec<String> = bag.errors().iter().map(parse_message).collect();
    parts.join("\n---\n")
}

/// Prepends recovered parse diagnostics before a later-stage error bag.
///
/// When the parser recovered with errors (`prior_parse` is `Some`), formats the parse bag and
/// joins it with `stage` (typically the resolve/type-check bag text). When no prior parse
/// errors exist, returns `stage` unchanged.
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
#[must_use]
pub fn prepend_parse_bag_styled(
    prior_parse: Option<&ParseBag>,
    stage: &str,
    source: Option<&str>,
    ctx: SpanContext<'_>,
    style: &dyn crate::render::DiagnosticStyle,
) -> String {
    match prior_parse {
        Some(bag) => join_diagnostics(
            style,
            &[
                format_parse_bag_styled(bag, source, ctx, style),
                stage.to_owned(),
            ],
        ),
        None => stage.to_owned(),
    }
}

/// Returns the human-readable message for a resolve error without codes or source snippets.
///
/// Resolves interned symbol indices through `names` for identifier and type names. Module I/O
/// and parse failures embed the underlying message or path context in the text.
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

/// Formats a resolve error as a full Cargo-style diagnostic.
///
/// [`ResolveError::DuplicateDefinition`] renders a secondary note at the first definition site
/// via [`render_diagnostic_with_note`]. Module errors override [`SpanContext::file_path`] with
/// the module path from the error. Uses [`PlainStyle`].
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
#[must_use]
pub fn format_resolve_error(source: &str, names: &impl SymbolNames, err: &ResolveError) -> String {
    format_resolve_error_styled(source, names, err, &PlainStyle, SpanContext::default())
}

/// Formats a resolve error with a caller-supplied style and file context.
///
/// See [`format_resolve_error`] for variant-specific behavior (duplicate-definition notes,
/// module path context). Delegates snippet rendering to [`render_diagnostic`] or
/// [`render_diagnostic_with_note`].
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
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

/// Returns the human-readable message for a type-check error without codes or source snippets.
///
/// Resolves interned symbol indices through `names` where the error stores a `symbol_index`.
/// Covers the full [`TypeCheckError`] variant set (ownership, traits, `?`, lang items, etc.).
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
        TypeCheckError::OverlappingMutBorrow { name, .. } => {
            format!("cannot borrow `{name}` as mutable more than once")
        }
        TypeCheckError::SharedMutBorrowConflict {
            name,
            new_borrow_is_mut,
            ..
        } => {
            if *new_borrow_is_mut {
                format!("cannot borrow `{name}` as mutable while it is borrowed")
            } else {
                format!("cannot borrow `{name}` as shared while it is mutably borrowed")
            }
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
        TypeCheckError::LangItemReserved { .. } => {
            "`#[lang_item]` is reserved for the standard library".to_owned()
        }
        TypeCheckError::LangItemDuplicate { kind, name, .. } => {
            format!("duplicate language item `{kind}` named `{name}`")
        }
        TypeCheckError::LangItemInvalid { detail, .. } => detail.clone(),
        TypeCheckError::GenericNestingTooDeep { depth, limit, .. } => {
            format!("generic type nesting too deep (depth {depth}, limit {limit})")
        }
    }
}

fn sym_label(names: &impl SymbolNames, symbol_index: u32) -> String {
    names
        .symbol_name(symbol_index)
        .map_or_else(|| format!("sym#{symbol_index}"), |name| format!("`{name}`"))
}

/// Formats a type-check error as a full Cargo-style diagnostic.
///
/// Includes secondary notes and help text from [`crate::type_notes::typecheck_ancillary`] (move
/// sites, type annotations, trait hints). Uses [`PlainStyle`] and default [`SpanContext`].
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
#[must_use]
pub fn format_typecheck_error(
    source: &str,
    names: &impl SymbolNames,
    err: &TypeCheckError,
) -> String {
    format_typecheck_error_styled(source, names, err, &PlainStyle, SpanContext::default())
}

/// Formats a type-check error with a caller-supplied style and file context.
///
/// When the error has a span, renders via [`render_diagnostic_enriched`] with ancillary notes
/// and help lines. Span-less errors emit only the header plus note/help labels.
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
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

/// Formats an IR validation error as a full Cargo-style diagnostic.
///
/// Uses [`IrError::to_string`] for the message and [`IrError::code`] for the label. Uses
/// [`PlainStyle`].
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
#[must_use]
pub fn format_ir_error(source: &str, err: &IrError) -> String {
    format_ir_error_styled(source, err, &PlainStyle, SpanContext::default())
}

/// Formats an IR validation error with a caller-supplied style and file context.
///
/// When [`IrError::span`] is present, renders via [`render_diagnostic`]; otherwise uses
/// [`DiagnosticStyle::error_header`] alone.
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
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

/// Formats a lowering error as a full Cargo-style diagnostic.
///
/// Uses [`LowerError::to_string`] for the message and [`LowerError::code`] for the label. Uses
/// [`PlainStyle`].
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
#[must_use]
pub fn format_lower_error(source: &str, err: &LowerError) -> String {
    format_lower_error_styled(source, err, &PlainStyle, SpanContext::default())
}

/// Formats a lowering error with a caller-supplied style and file context.
///
/// When [`LowerError::span`] is present, renders via [`render_diagnostic`]; otherwise uses
/// [`DiagnosticStyle::error_header`] alone.
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
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

/// Renders `message` at `span` with a secondary note at `note_span`.
///
/// Convenience wrapper around [`render_diagnostic_with_note`] for tests and ad hoc diagnostics.
/// Uses the generic placeholder code `E0000`, [`PlainStyle`], and default [`SpanContext`].
///
/// # Panics
///
/// Never panics on malformed user input or out-of-range spans.
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
    use crate::ParseBag;
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

    #[test]
    fn parse_bag_messages_without_lex_prefix() {
        let bag = ParseBag::from_errors(vec![
            ParseError::Lex(LexError::UnterminatedString { start: 0 }),
            ParseError::UnexpectedEof {
                expected: ExpectedToken::Punct("}"),
                span: Span::new(10, 10),
            },
        ]);
        let messages = format_parse_bag_messages(&bag);
        assert!(!messages.contains("lex error:"));
        assert!(messages.contains("unterminated byte string"));
        assert!(messages.contains("expected }, found end of file"));
        assert!(messages.contains("\n---\n"));
    }
}

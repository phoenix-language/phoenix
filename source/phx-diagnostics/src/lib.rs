//! Phoenix diagnostics — spans, labels, and user-facing error reporting.
//!
//! Compiler passes return structured errors with [`Span`]s; formatters render source lines and
//! carets for the CLI. Spans are byte offsets into a specific module's source buffer; use
//! [`LocatedError`] until spans carry a file id.
//!
//! ## Modules
//!
//! - `span` (private) — byte-offset [`Span`] into UTF-8 source.
//! - `code` — stable [`DiagnosticCode`] labels.
//! - `located` — [`LocatedError`] tying an error to a module id.
//! - `lex_error` — lexer failures ([`LexError`]).
//! - `parse_error` — parser failures ([`ParseError`], [`ExpectedToken`]).
//! - `resolve_error` — name resolution ([`ResolveError`], [`DiagnosticBag`]).
//! - `type_error` — type checking ([`TypeCheckError`], [`TypeCheckBag`]).
//! - `lower_error` — IR lowering ([`LowerError`], [`LowerBag`]).
//! - `ir_error` — IR validation ([`IrError`], [`IrBag`]).

mod code;
mod explain;
mod format;
mod ir_error;
mod lex_error;
mod lint;
mod located;
mod lower_error;
mod parse_error;
mod render;
mod resolve_error;
mod span;
mod symbol_names;
mod type_error;
mod type_error_registry;

#[cfg(test)]
mod explain_coverage;
mod type_notes;

pub use code::DiagnosticCode;
pub use explain::{lookup as explain_code, normalize_code};
pub use format::{
    format_ir_error, format_ir_error_styled, format_lex_error, format_lex_error_styled,
    format_lower_error, format_lower_error_styled, format_parse_bag_messages,
    format_parse_bag_styled, format_parse_error, format_parse_error_styled, format_resolve_error,
    format_resolve_error_styled, format_span_message, format_span_message_with_note,
    format_typecheck_error, format_typecheck_error_styled, parse_message, prepend_parse_bag_styled,
    resolve_message, typecheck_message,
};
pub use ir_error::{IrBag, IrError, IrResult};
pub use lex_error::LexError;
pub use lint::{
    Lint, LintBag, LintDenyConfig, LintKind, LocatedLint, count_denied_lints, parse_lint_name,
};
pub use located::LocatedError;
pub use lower_error::{LowerBag, LowerError, LowerResult};
pub use parse_error::{ExpectedToken, ParseBag, ParseError, ParseResult};
pub use render::{
    AncillaryNote, DiagnosticAncillary, DiagnosticStyle, PlainStyle, SpanContext,
    diagnostic_display_path, format_lints_styled, join_diagnostics, line_col, render_diagnostic,
    render_diagnostic_enriched, render_diagnostic_with_note, render_lint,
};
pub use resolve_error::{DiagnosticBag, InvalidMainReason, ResolveError, ResolveResult};
pub use span::Span;
pub use symbol_names::SymbolNames;
pub use type_error::{MismatchKind, TypeCheckBag, TypeCheckError, TypeCheckResult};

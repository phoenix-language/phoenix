//! Phoenix diagnostics — spans, labels, and user-facing error reporting.
//!
//! Compiler passes return structured errors with [`Span`]s; formatters turn those into
//! Cargo-style snippets (path, line/column, caret) for the CLI and golden tests.
//!
//! ## Spans and module identity
//!
//! [`Span`] is a half-open byte range into one module's UTF-8 source buffer. Multi-file crates
//! wrap errors in [`LocatedError`] until spans carry a file id.
//!
//! ## Public API overview
//!
//! | Layer | Role | Key types |
//! |-------|------|-----------|
//! | Errors | Structured failures per pass | [`LexError`], [`ParseError`], [`ResolveError`], [`TypeCheckError`], [`LowerError`], [`IrError`] |
//! | Codes | Stable `E####` / `W####` labels | [`DiagnosticCode`], `{Error}::code()` |
//! | Messages | Human-readable text (no snippet) | [`format_lex_error_styled`], [`format_typecheck_error_styled`], … |
//! | Rendering | Snippet + header + location line | [`render_diagnostic`], [`DiagnosticStyle`], [`PlainStyle`] |
//! | Explain | Static text for `phx explain` | [`explain_code`], [`normalize_code`] |
//! | Lints | Warnings with optional deny | [`Lint`], [`LintBag`], [`format_lints_styled`] |
//!
//! Typical CLI flow: resolve a message with `*_message` or `format_*_styled`, then optionally
//! wrap with [`render_diagnostic_enriched`] when a source buffer and display path are available.
//!
//! ## Diagnostic code registries
//!
//! Each error enum exposes [`DiagnosticCode`] via a `code()` method. Type-check variants map
//! through the generated table in `type_error_registry.rs` (E2001–E2046); other passes implement
//! `code()` on their enum directly (E0xxx lex, E1xxx resolve, E3xxx parse, E4xxx lower/IR).
//! Every registered code should have a matching entry in [`explain_code`] for `phx explain`.
//!
//! ## Internal modules
//!
//! - `span` — byte-offset [`Span`] into UTF-8 source.
//! - `code` — stable [`DiagnosticCode`] labels.
//! - `located` — [`LocatedError`] tying an error to a module id.
//! - `format` — message formatters (re-exported at crate root).
//! - `render` — snippet rendering (re-exported at crate root).
//! - `explain` — code normalization and lookup (re-exported as [`explain_code`]).
//! - `type_error_registry` — generated [`TypeCheckError::code`] / [`TypeCheckError::span`].

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

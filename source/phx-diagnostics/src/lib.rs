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

mod code;
mod format;
mod lex_error;
mod located;
mod lower_error;
mod parse_error;
mod resolve_error;
mod span;
mod symbol_names;
mod type_error;

pub use code::DiagnosticCode;
pub use format::{
    format_lex_error, format_lower_error, format_resolve_error, format_span_message,
    format_span_message_with_note, format_typecheck_error, resolve_message, typecheck_message,
};
pub use lex_error::LexError;
pub use located::LocatedError;
pub use lower_error::{LowerBag, LowerError, LowerResult};
pub use parse_error::{ExpectedToken, ParseBag, ParseError, ParseResult};
pub use resolve_error::{DiagnosticBag, InvalidMainReason, ResolveError, ResolveResult};
pub use span::Span;
pub use symbol_names::SymbolNames;
pub use type_error::{TypeCheckBag, TypeCheckError, TypeCheckResult};

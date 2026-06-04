//! Phoenix diagnostics — spans, labels, and user-facing error reporting.
//!
//! Compiler passes return structured errors with [`Span`]s; later formatting will render
//! source lines and carets in the CLI.
//!
//! ## Modules
//!
//! - `span` (private) — byte-offset [`Span`] into UTF-8 source.
//! - `lex_error` — lexer failures ([`LexError`]).
//! - `parse_error` — parser failures ([`ParseError`], [`ExpectedToken`]).
//! - `resolve_error` — name resolution ([`ResolveError`], [`DiagnosticBag`]).
//! - `type_error` — type checking ([`TypeCheckError`], [`TypeCheckBag`]).

mod format;
mod lex_error;
mod parse_error;
mod resolve_error;
mod span;
mod type_error;

pub use format::{format_span_message, format_span_message_with_note, format_typecheck_error};
pub use lex_error::LexError;
pub use parse_error::{ExpectedToken, ParseBag, ParseError, ParseResult};
pub use resolve_error::{DiagnosticBag, InvalidMainReason, ResolveError, ResolveResult};
pub use span::Span;
pub use type_error::{TypeCheckBag, TypeCheckError, TypeCheckResult};

//! Phoenix diagnostics — spans, labels, and user-facing error reporting.

mod lex_error;
mod span;

pub use lex_error::LexError;
pub use span::Span;

//! Phoenix diagnostics — spans, labels, and user-facing error reporting.

mod lex_error;
mod parse_error;
mod span;

pub use lex_error::LexError;
pub use parse_error::{ExpectedToken, ParseError, ParseResult};
pub use span::Span;

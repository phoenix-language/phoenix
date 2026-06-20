//! Lexical failure types for the Phoenix syntax pipeline.
//!
//! Produced by [`phx_syntax::lex`] and the internal [`phx_syntax::Lexer`] while scanning source
//! text into tokens. Lexical errors are **fatal for the current compilation unit**: the lexer
//! stops tokenizing and returns the first failure; unlike parse errors, there is no recovery bag
//! at this stage.
//!
//! ## Compiler pass
//!
//! Lexing is the first syntax-stage pass. Failures surface directly as [`LexError`] or, when the
//! parser drives the lexer inline, as [`ParseError::Lex`](crate::ParseError::Lex) so callers see
//! one error enum for the syntax stage. Resolve, type-check, and later passes never emit lex
//! errors.
//!
//! ## Diagnostic codes (E0001–E0009)
//!
//! | Code | Variant | Summary |
//! |------|---------|---------|
//! | E0001 | [`LexError::UnterminatedString`] | Byte string or byte character literal missing closing quote |
//! | E0002 | [`LexError::UnterminatedBlockComment`] | `///` block comment not closed with `///` |
//! | E0003 | [`LexError::InvalidEscape`] | Invalid escape sequence inside a byte literal |
//! | E0004 | [`LexError::UnexpectedChar`] | Character cannot start any token |
//! | E0005 | [`LexError::IntegerOverflow`] | Integer literal exceeds representable range |
//! | E0006 | [`LexError::InvalidInt`] | Malformed integer literal (radix, digits, empty prefix) |
//! | E0007 | [`LexError::InvalidFloat`] | Malformed or out-of-range float literal |
//! | E0008 | [`LexError::LexemeTooLong`] | Identifier or literal exceeds internal length limits |
//! | E0009 | [`LexError::InvalidUtf8`] | String literal bytes are not valid UTF-8 |
//!
//! ## Integration with [`crate::format`]
//!
//! - **Message text** — [`format_lex_error_styled`] uses the same user-facing prose as
//!   [`LexError`]'s [`Display`] impl (without diagnostic codes or snippets in the message alone).
//! - **Single error** — [`format_lex_error`] / [`format_lex_error_styled`] render a Cargo-style
//!   header plus caret when [`LexError::span`] returns a range and source text is available.
//! - **Parse wrapper** — [`format_parse_error_styled`] delegates [`ParseError::Lex`](crate::ParseError::Lex)
//!   to [`format_lex_error_styled`] when source is present.
//!
//! [`format_lex_error`]: crate::format_lex_error
//! [`format_lex_error_styled`]: crate::format_lex_error_styled
//! [`format_parse_error_styled`]: crate::format_parse_error_styled

use core::fmt;

use crate::Span;
use crate::code::DiagnosticCode;

/// A lexical error produced while tokenizing Phoenix source.
///
/// Each variant maps to a stable [`DiagnosticCode`] via [`LexError::code`] (E0001–E0009). Primary
/// source locations are available through [`LexError::span`] for caret rendering in
/// [`crate::format::format_lex_error`].
///
/// The enum is [`non_exhaustive`] so new lexeme forms can add variants without breaking callers
/// outside this crate on minor compiler updates.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LexError {
    /// A byte string or byte character literal was not closed before end of input.
    ///
    /// Emitted when the opening `b"` or `b'` quote has no matching closing quote. The span is a
    /// zero-width caret at the opening quote ([`LexError::span`] uses `start..start`).
    UnterminatedString {
        /// Byte offset of the opening quote.
        start: u32,
    },
    /// A `///` block comment was not closed with a matching `///` before end of input.
    ///
    /// Phoenix block comments are delimited by `///` on their own lines, not `/* … */`. The span
    /// is a zero-width caret at the opening delimiter.
    UnterminatedBlockComment {
        /// Byte offset of the opening `///`.
        start: u32,
    },
    /// An invalid escape sequence inside a byte string or byte character literal.
    ///
    /// The backslash is not followed by a recognized escape (for example `\n`, `\xHH`). The span
    /// is a zero-width caret at the backslash.
    InvalidEscape {
        /// Byte offset of the backslash starting the escape.
        offset: u32,
    },
    /// A character that cannot start any Phoenix token.
    ///
    /// Typically stray punctuation or a Unicode character outside the allowed identifier /
    /// literal grammar. The span is a zero-width caret at the offending byte.
    UnexpectedChar {
        /// The offending character (decoded from UTF-8 at `offset`).
        ch: char,
        /// Byte offset in the source.
        offset: u32,
    },
    /// Integer literal does not fit in the parsed representation.
    ///
    /// The lexeme is syntactically valid but its numeric value exceeds what the compiler accepts
    /// for that literal form. The span covers the full lexeme byte range.
    IntegerOverflow {
        /// Inclusive start byte offset of the lexeme.
        start: u32,
        /// Exclusive end byte offset of the lexeme.
        end: u32,
    },
    /// Integer literal is malformed (empty radix prefix, invalid digits, etc.).
    ///
    /// Covers invalid binary/hex/octal prefixes, empty numeric bodies, and digit sequences that
    /// do not match Phoenix integer grammar. The span covers the full lexeme byte range.
    InvalidInt {
        /// Inclusive start byte offset of the lexeme.
        start: u32,
        /// Exclusive end byte offset of the lexeme.
        end: u32,
    },
    /// Float literal is malformed or out of range.
    ///
    /// Covers invalid exponent parts, multiple decimal points, and values outside the accepted
    /// float range. The span covers the full lexeme byte range.
    InvalidFloat {
        /// Inclusive start byte offset of the lexeme.
        start: u32,
        /// Exclusive end byte offset of the lexeme.
        end: u32,
    },
    /// String literal bytes are not valid UTF-8.
    ///
    /// Phoenix string literals (`"…"`) must decode as UTF-8; byte literals use the `b"…"` form
    /// instead. The span covers the full literal including quotes when known.
    InvalidUtf8 {
        /// Inclusive start byte offset of the literal (opening quote).
        start: u32,
        /// Exclusive end byte offset of the literal (closing quote or error site).
        end: u32,
    },
    /// Identifier or literal exceeds internal length limits.
    ///
    /// Protects the compiler from unbounded lexeme buffers. The span covers the full lexeme byte
    /// range.
    LexemeTooLong {
        /// Inclusive start byte offset of the lexeme.
        start: u32,
        /// Exclusive end byte offset of the lexeme.
        end: u32,
    },
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnterminatedString { start } => {
                write!(
                    f,
                    "unterminated byte string or character literal at offset {start}"
                )
            }
            Self::UnterminatedBlockComment { start } => {
                write!(f, "unterminated block comment at offset {start}")
            }
            Self::InvalidEscape { offset } => {
                write!(f, "invalid escape sequence at offset {offset}")
            }
            Self::UnexpectedChar { ch, offset } => {
                write!(f, "unexpected character {ch:?} at offset {offset}")
            }
            Self::IntegerOverflow { start, end } => {
                write!(f, "integer literal overflow at bytes {start}..{end}")
            }
            Self::InvalidInt { start, end } => {
                write!(f, "invalid integer literal at bytes {start}..{end}")
            }
            Self::InvalidFloat { start, end } => {
                write!(f, "invalid float literal at bytes {start}..{end}")
            }
            Self::InvalidUtf8 { start, end } => {
                write!(f, "invalid UTF-8 in string literal at bytes {start}..{end}")
            }
            Self::LexemeTooLong { start, end } => {
                write!(f, "lexeme too long at bytes {start}..{end}")
            }
        }
    }
}

impl std::error::Error for LexError {}

impl LexError {
    /// Stable diagnostic code for this error.
    ///
    /// Maps each variant to E0001–E0009. Used by formatters, golden tests, and `phx explain`.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::UnterminatedString { .. } => DiagnosticCode::new("E0001"),
            Self::UnterminatedBlockComment { .. } => DiagnosticCode::new("E0002"),
            Self::InvalidEscape { .. } => DiagnosticCode::new("E0003"),
            Self::UnexpectedChar { .. } => DiagnosticCode::new("E0004"),
            Self::IntegerOverflow { .. } => DiagnosticCode::new("E0005"),
            Self::InvalidInt { .. } => DiagnosticCode::new("E0006"),
            Self::InvalidFloat { .. } => DiagnosticCode::new("E0007"),
            Self::InvalidUtf8 { .. } => DiagnosticCode::new("E0009"),
            Self::LexemeTooLong { .. } => DiagnosticCode::new("E0008"),
        }
    }

    /// Source span for caret rendering when the error refers to a lexeme range.
    ///
    /// Returns `Some` for every variant today: point errors use a zero-width span at the
    /// offending byte; lexeme-range errors use `start..end`. Formatters fall back to a
    /// header-only line when this is `None` (reserved for future variants).
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::UnterminatedString { start } | Self::UnterminatedBlockComment { start } => {
                Some(Span::new(*start, *start))
            }
            Self::InvalidEscape { offset } | Self::UnexpectedChar { offset, .. } => {
                Some(Span::new(*offset, *offset))
            }
            Self::IntegerOverflow { start, end }
            | Self::InvalidInt { start, end }
            | Self::InvalidFloat { start, end }
            | Self::InvalidUtf8 { start, end }
            | Self::LexemeTooLong { start, end } => Some(Span::new(*start, *end)),
        }
    }
}

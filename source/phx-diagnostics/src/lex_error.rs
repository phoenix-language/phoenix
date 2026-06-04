//! Lexer failure types.
//!
//! Returned from [`crate::LexError`] via [`phx_syntax::lex`] and the [`phx_syntax::Lexer`].

use core::fmt;

use crate::Span;
use crate::code::DiagnosticCode;

/// A lexical error produced while tokenizing Phoenix source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexError {
    /// A byte string or byte character literal was not closed.
    UnterminatedString {
        /// Start offset of the opening quote.
        start: u32,
    },
    /// A `///` block comment was not closed with `///`.
    UnterminatedBlockComment {
        /// Start offset of the opening `///`.
        start: u32,
    },
    /// An invalid escape sequence inside a byte literal.
    InvalidEscape {
        /// Offset of the backslash starting the escape.
        offset: u32,
    },
    /// A character that cannot start any token.
    UnexpectedChar {
        /// The offending character.
        ch: char,
        /// Byte offset in the source.
        offset: u32,
    },
    /// Integer literal does not fit in the parsed representation.
    IntegerOverflow {
        /// Inclusive start byte offset of the lexeme.
        start: u32,
        /// Exclusive end byte offset of the lexeme.
        end: u32,
    },
    /// Integer literal is malformed (empty radix prefix, invalid digits, etc.).
    InvalidInt {
        /// Inclusive start byte offset of the lexeme.
        start: u32,
        /// Exclusive end byte offset of the lexeme.
        end: u32,
    },
    /// Float literal is malformed or out of range.
    InvalidFloat {
        /// Inclusive start byte offset of the lexeme.
        start: u32,
        /// Exclusive end byte offset of the lexeme.
        end: u32,
    },
    /// Identifier or literal exceeds internal limits.
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
            Self::LexemeTooLong { start, end } => {
                write!(f, "lexeme too long at bytes {start}..{end}")
            }
        }
    }
}

impl std::error::Error for LexError {}

impl LexError {
    /// Stable diagnostic code for this error.
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
            Self::LexemeTooLong { .. } => DiagnosticCode::new("E0008"),
        }
    }

    /// Source span for caret rendering when the error refers to a lexeme range.
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
            | Self::LexemeTooLong { start, end } => Some(Span::new(*start, *end)),
        }
    }
}

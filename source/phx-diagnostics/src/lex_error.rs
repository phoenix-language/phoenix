//! Lexer failure types.
//!
//! Returned from [`crate::LexError`] via [`phx_syntax::lex`] and the [`phx_syntax::Lexer`].

use core::fmt;

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

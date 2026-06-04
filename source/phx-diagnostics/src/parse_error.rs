//! Parser failure types.
//!
//! Returned from [`ParseError`] via [`phx_syntax::parse`] and the internal [`phx_syntax::parser::Parser`].

use core::fmt;
use std::borrow::Cow;

use crate::LexError;
use crate::Span;

/// Human-readable description of an expected token class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedToken {
    /// Any token except end of input.
    Token,
    /// A specific punctuation or operator terminal.
    Punct(&'static str),
    /// A `snake_case` identifier.
    Ident,
    /// A `PascalCase` type identifier.
    TypeIdent,
    /// A literal token.
    Literal,
    /// A type expression.
    Type,
    /// An expression.
    Expr,
    /// A pattern.
    Pattern,
    /// A statement.
    Stmt,
}

impl fmt::Display for ExpectedToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Token => f.write_str("token"),
            Self::Punct(s) => f.write_str(s),
            Self::Ident => f.write_str("identifier"),
            Self::TypeIdent => f.write_str("type identifier"),
            Self::Literal => f.write_str("literal"),
            Self::Type => f.write_str("type"),
            Self::Expr => f.write_str("expression"),
            Self::Pattern => f.write_str("pattern"),
            Self::Stmt => f.write_str("statement"),
        }
    }
}

/// A parse error produced while building the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Lexical error from the lexer.
    Lex(LexError),
    /// Found a token that does not match the current grammar rule.
    UnexpectedToken {
        /// What the parser expected.
        expected: ExpectedToken,
        /// Short description of what was found (borrowed when static, owned for dynamic lexemes).
        found: Cow<'static, str>,
        /// Span of the unexpected token.
        span: Span,
    },
    /// Input ended before a required token.
    UnexpectedEof {
        /// What the parser still expected.
        expected: ExpectedToken,
        /// Span where parsing stopped (often EOF position).
        span: Span,
    },
    /// Syntax that is in the grammar but not supported in this compiler milestone.
    UnsupportedSyntax {
        /// Stable feature name for tests and diagnostics.
        feature: &'static str,
        /// Span covering the unsupported construct.
        span: Span,
    },
    /// A pattern could not be parsed.
    InvalidPattern {
        /// Span of the invalid pattern.
        span: Span,
    },
    /// The identifier intern table is full (`u32` index space exhausted).
    InternTableFull {
        /// Span of the identifier being interned.
        span: Span,
    },
}

impl ParseError {
    /// Returns the primary span for this error, if any.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        match self {
            Self::Lex(e) => match e {
                LexError::UnterminatedString { start }
                | LexError::UnterminatedBlockComment { start } => Some(Span::new(*start, *start)),
                LexError::InvalidEscape { offset } | LexError::UnexpectedChar { offset, .. } => {
                    Some(Span::new(*offset, *offset))
                }
                LexError::IntegerOverflow { start, end }
                | LexError::InvalidInt { start, end }
                | LexError::InvalidFloat { start, end }
                | LexError::LexemeTooLong { start, end } => Some(Span::new(*start, *end)),
            },
            Self::UnexpectedToken { span, .. }
            | Self::UnexpectedEof { span, .. }
            | Self::UnsupportedSyntax { span, .. }
            | Self::InvalidPattern { span, .. }
            | Self::InternTableFull { span, .. } => Some(*span),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(e) => write!(f, "lex error: {e}"),
            Self::UnexpectedToken {
                expected, found, ..
            } => write!(f, "expected {expected}, found {found}"),
            Self::UnexpectedEof { expected, .. } => {
                write!(f, "expected {expected}, found end of file")
            }
            Self::UnsupportedSyntax { feature, .. } => {
                write!(f, "unsupported syntax: {feature}")
            }
            Self::InvalidPattern { .. } => f.write_str("invalid pattern"),
            Self::InternTableFull { .. } => f.write_str("identifier intern table is full"),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Lex(e) => Some(e),
            Self::UnexpectedToken { .. }
            | Self::UnexpectedEof { .. }
            | Self::UnsupportedSyntax { .. }
            | Self::InvalidPattern { .. }
            | Self::InternTableFull { .. } => None,
        }
    }
}

/// Parse output that may carry non-fatal errors alongside a partial value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseResult<T> {
    /// Parsed value when the parser produced one.
    pub value: T,
    /// Non-fatal errors collected during parsing.
    pub errors: Vec<ParseError>,
}

impl<T> ParseResult<T> {
    /// Creates a successful result with no errors.
    #[must_use]
    pub const fn ok(value: T) -> Self {
        Self {
            value,
            errors: Vec::new(),
        }
    }

    /// Returns `true` if any errors were recorded.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Collected parse diagnostics; parsing may continue after non-fatal errors.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParseBag {
    errors: Vec<ParseError>,
}

impl ParseBag {
    /// Creates an empty bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a single error (convenience for lex failures).
    #[must_use]
    pub fn from_single(error: ParseError) -> Self {
        Self {
            errors: vec![error],
        }
    }

    /// Records an error.
    pub fn push(&mut self, error: ParseError) {
        self.errors.push(error);
    }

    /// Returns `true` if any errors were recorded.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Borrows collected errors.
    #[must_use]
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// Consumes the bag and returns errors.
    #[must_use]
    pub fn into_errors(self) -> Vec<ParseError> {
        self.errors
    }
}

impl fmt::Display for ParseBag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, e) in self.errors.iter().enumerate() {
            if i > 0 {
                f.write_str("\n---\n")?;
            }
            write!(f, "{e}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseBag {}

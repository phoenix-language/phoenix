//! Parser failure types for the Phoenix syntax pipeline.
//!
//! Produced by [`phx_syntax::parse`] and the internal [`phx_syntax::parser::Parser`] while building
//! the AST. The parser may recover after non-fatal errors and continue scanning the remainder of
//! the file; recovered errors are collected in [`ParseBag`] or returned alongside a partial value
//! in [`ParseResult`].
//!
//! ## Compiler pass
//!
//! Parse follows lexing ([`LexError`]) and precedes name resolution ([`ResolveError`]). When the
//! parser drives the lexer inline, lexical failures are wrapped as [`ParseError::Lex`] so callers
//! see a single error enum for the syntax stage.
//!
//! ## Diagnostic codes (E3001–E3005)
//!
//! | Code | Variant | Summary |
//! |------|---------|---------|
//! | E3001 | [`ParseError::UnexpectedToken`] | Token does not match the current grammar rule |
//! | E3002 | [`ParseError::UnexpectedEof`] | Input ended before a required token |
//! | E3003 | [`ParseError::UnsupportedSyntax`] | Construct is in the grammar but not in this milestone |
//! | E3004 | [`ParseError::InvalidPattern`] | Pattern could not be parsed |
//! | E3005 | [`ParseError::InternTableFull`] | Identifier intern table exhausted (`u32` index space) |
//!
//! [`ParseError::Lex`] delegates [`ParseError::code`] to the nested [`LexError::code`] (E0001–E0009).
//!
//! ## Integration with [`crate::format`]
//!
//! - **Message text** — [`parse_message`] mirrors [`ParseError`]'s [`Display`] output without
//!   diagnostic codes or source snippets.
//! - **Single error** — [`format_parse_error`] / [`format_parse_error_styled`] render a
//!   Cargo-style header plus caret when source and span are available.
//! - **Multiple errors** — [`format_parse_bag_styled`] joins bag entries; [`format_parse_bag_messages`]
//!   returns compact `\n---\n`-separated text.
//! - **Cross-pass output** — [`prepend_parse_bag_styled`] prepends recovered parse diagnostics
//!   before resolve or type-check bag text in the CLI.

use core::fmt;
use std::borrow::Cow;

use crate::LexError;
use crate::Span;
use crate::code::DiagnosticCode;

/// Human-readable description of an expected token class for parse diagnostics.
///
/// Used in [`ParseError::UnexpectedToken`] and [`ParseError::UnexpectedEof`] messages. Displayed
/// via [`ExpectedToken`]'s [`Display`] impl (for example `"identifier"`, `"}"`, `"expression"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedToken {
    /// Any token except end of input.
    Token,
    /// A specific punctuation or operator terminal (for example `"}""`, `"::"`).
    Punct(&'static str),
    /// A `snake_case` identifier.
    Ident,
    /// A `PascalCase` type identifier.
    TypeIdent,
    /// A literal token (integer, float, string, or character).
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
///
/// Each variant maps to a stable [`DiagnosticCode`] via [`ParseError::code`] (E3001–E3005, or the
/// nested lex code for [`ParseError::Lex`]). Primary source locations are available through
/// [`ParseError::span`] for caret rendering in [`crate::format::format_parse_error`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    /// Lexical error from the lexer, wrapped so the syntax stage exposes one error type.
    ///
    /// Message text and code come from the nested [`LexError`] via [`parse_message`] and
    /// [`ParseError::code`].
    Lex(LexError),
    /// Found a token that does not match the current grammar rule.
    ///
    /// Emitted when the parser's lookahead does not satisfy the active production. The `found`
    /// string is a short human label (often the token's display name or lexeme).
    UnexpectedToken {
        /// What the parser expected at this point.
        expected: ExpectedToken,
        /// Short description of what was found (borrowed when static, owned for dynamic lexemes).
        found: Cow<'static, str>,
        /// Span of the unexpected token.
        span: Span,
    },
    /// Input ended before a required token.
    ///
    /// Distinct from [`ParseError::UnexpectedToken`] because the caret typically points at EOF
    /// and the message reads "found end of file".
    UnexpectedEof {
        /// What the parser still expected when input ran out.
        expected: ExpectedToken,
        /// Span where parsing stopped (often the EOF position).
        span: Span,
    },
    /// Syntax that is in the grammar but not supported in this compiler milestone.
    ///
    /// The `feature` string is a stable identifier for tests, `phx explain`, and diagnostic
    /// goldens (not user-facing prose).
    UnsupportedSyntax {
        /// Stable feature name for tests and diagnostics.
        feature: &'static str,
        /// Span covering the unsupported construct.
        span: Span,
    },
    /// A pattern could not be parsed in a `match`, `let`, or binding position.
    InvalidPattern {
        /// Span of the invalid pattern.
        span: Span,
    },
    /// The identifier intern table is full (`u32` index space exhausted).
    ///
    /// Indicates an internal resource limit rather than a user syntax mistake; still reported
    /// at the identifier's source span.
    InternTableFull {
        /// Span of the identifier being interned when the table overflowed.
        span: Span,
    },
}

impl ParseError {
    /// Stable diagnostic code for this error.
    ///
    /// Returns E3001–E3005 for parse-native variants; [`ParseError::Lex`] forwards to
    /// [`LexError::code`].
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::Lex(e) => e.code(),
            Self::UnexpectedToken { .. } => DiagnosticCode::new("E3001"),
            Self::UnexpectedEof { .. } => DiagnosticCode::new("E3002"),
            Self::UnsupportedSyntax { .. } => DiagnosticCode::new("E3003"),
            Self::InvalidPattern { .. } => DiagnosticCode::new("E3004"),
            Self::InternTableFull { .. } => DiagnosticCode::new("E3005"),
        }
    }

    /// Returns the primary span for caret rendering, if any.
    ///
    /// For [`ParseError::Lex`], delegates to the nested error's span rules (point spans for
    /// unterminated literals, lexeme ranges for numeric and string failures). All other variants
    /// return their explicit `span` field.
    ///
    /// # Panics
    ///
    /// Never panics.
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
                | LexError::InvalidUtf8 { start, end }
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
///
/// The parser fills `errors` while still producing an AST fragment in `value` when recovery
/// succeeds. Call [`ParseResult::has_errors`] before treating the parse as clean; use
/// [`ParseResult::into_bag`] or [`ParseResult::errors_bag`] to hand errors to
/// [`crate::format::format_parse_bag_styled`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseResult<T> {
    /// Parsed value when the parser produced one (possibly partial after recovery).
    pub value: T,
    /// Non-fatal errors collected during parsing.
    pub errors: Vec<ParseError>,
}

impl<T> ParseResult<T> {
    /// Creates a successful result with no errors.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub const fn ok(value: T) -> Self {
        Self {
            value,
            errors: Vec::new(),
        }
    }

    /// Returns `true` if any errors were recorded during parsing.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Creates a result carrying `value` and collected `errors`.
    ///
    /// Used when the parser recovered and wants to return both the partial AST and diagnostics.
    #[must_use]
    pub fn with_errors(value: T, errors: Vec<ParseError>) -> Self {
        Self { value, errors }
    }

    /// Moves collected errors into a [`ParseBag`] for formatting or merging with other passes.
    #[must_use]
    pub fn into_bag(self) -> ParseBag {
        ParseBag::from_errors(self.errors)
    }

    /// Clones collected errors into a [`ParseBag`] without consuming this result.
    #[must_use]
    pub fn errors_bag(&self) -> ParseBag {
        ParseBag::from_errors(self.errors.clone())
    }
}

/// Collected parse diagnostics; parsing may continue after non-fatal errors.
///
/// Unlike fatal `Result` returns, a [`ParseBag`] lets the parser finish the file and report every
/// syntax issue in one pass. Format with [`crate::format::format_parse_bag_styled`] or
/// [`crate::format::format_parse_bag_messages`].
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

    /// Creates a bag containing a single error.
    ///
    /// Convenience for early-exit paths (for example a fatal lex failure before AST construction).
    #[must_use]
    pub fn from_single(error: ParseError) -> Self {
        Self {
            errors: vec![error],
        }
    }

    /// Creates a bag from an existing error vector.
    #[must_use]
    pub fn from_errors(errors: Vec<ParseError>) -> Self {
        Self { errors }
    }

    /// Appends an error to the bag.
    pub fn push(&mut self, error: ParseError) {
        self.errors.push(error);
    }

    /// Returns `true` if any errors were recorded.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Borrows collected errors in insertion order.
    #[must_use]
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// Consumes the bag and returns the underlying error vector.
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

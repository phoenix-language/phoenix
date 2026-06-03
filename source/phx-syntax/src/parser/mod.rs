//! Recursive-descent parser for Phoenix source.
//!
//! The [`Parser`] holds the token cursor, source slice, and [`Interner`]. Submodules split the
//! grammar by syntactic category (`decl`, `expr`, `stmt`, `pat`, `types`). Entry point:
//! [`parse`].

mod decl;
mod expr;
mod pat;
mod stmt;
mod types;

use std::borrow::Cow;

use phx_diagnostics::{ExpectedToken, ParseError, Span};

use crate::ast::Program;
use crate::intern::Interner;
use crate::lexer::lex;
use crate::source_file::SourceFile;
use crate::token::{Keyword, Token, TokenKind};

/// Parser over a token stream and source text.
pub(crate) struct Parser<'src> {
    /// Original source buffer (for spans and literal text).
    pub(crate) source: &'src str,
    /// Token stream from [`crate::lexer::lex`].
    pub(crate) tokens: &'src [Token<'src>],
    /// Index of the current token in `tokens`.
    pub(crate) pos: usize,
    /// Intern table filled while parsing identifiers.
    pub(crate) interner: Interner,
}

impl<'src> Parser<'src> {
    /// Builds a parser over `tokens` borrowed from `source`.
    pub(crate) fn new(source: &'src str, tokens: &'src [Token<'src>]) -> Self {
        Self::with_interner(source, tokens, Interner::new())
    }

    /// Builds a parser that interns identifiers into `interner`.
    pub(crate) fn with_interner(
        source: &'src str,
        tokens: &'src [Token<'src>],
        interner: Interner,
    ) -> Self {
        Self {
            source,
            tokens,
            pos: 0,
            interner,
        }
    }

    /// Interns `text` as a value identifier (`snake_case` name).
    pub(crate) fn intern_ident(&mut self, text: &str) -> crate::ast::Ident {
        crate::ast::Ident {
            symbol: self.interner.intern(text),
        }
    }

    /// Interns `text` as a type identifier (`PascalCase` name).
    pub(crate) fn intern_type_name(&mut self, text: &str) -> crate::ast::TypeName {
        crate::ast::TypeName {
            symbol: self.interner.intern(text),
        }
    }

    /// Returns `true` when the cursor is at EOF.
    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.tokens.len() || matches!(self.peek_kind(), TokenKind::Eof)
    }

    /// Returns the current token without advancing.
    pub(crate) fn peek(&self) -> Option<&Token<'src>> {
        self.tokens.get(self.pos)
    }

    /// Returns the kind of the current token (or [`TokenKind::Eof`]).
    pub(crate) fn peek_kind(&self) -> TokenKind<'src> {
        self.peek().map_or(TokenKind::Eof, |t| t.kind.clone())
    }

    /// Returns the token kind at `pos + n` without consuming.
    pub(crate) fn peek_at(&self, n: usize) -> TokenKind<'src> {
        self.tokens
            .get(self.pos + n)
            .map_or(TokenKind::Eof, |t| t.kind.clone())
    }

    /// Returns `true` when the current token is `{` and the following tokens look like a struct literal body (`..`, `field:`, or empty only for type names), not a block or `match` arms.
    ///
    /// `type_name` is `true` when the path began with a [`TokenKind::TypeIdent`] (e.g. `Point {}`); value idents use `foo { }` for blocks, not empty struct literals.
    pub(crate) fn brace_starts_struct_literal_body(&self, type_name: bool) -> bool {
        if !matches!(self.peek_kind(), TokenKind::LBrace) {
            return false;
        }
        match self.peek_at(1) {
            TokenKind::RBrace => type_name,
            TokenKind::DotDot => true,
            TokenKind::Ident(_) => matches!(self.peek_at(2), TokenKind::Colon),
            _ => false,
        }
    }

    /// Consumes and returns the current token.
    pub(crate) fn bump(&mut self) -> Option<&Token<'src>> {
        if self.at_end() {
            return None;
        }
        let t = &self.tokens[self.pos];
        self.pos += 1;
        Some(t)
    }

    /// Span of the current token, or EOF position in `source`.
    pub(crate) fn current_span(&self) -> Span {
        self.peek().map_or_else(
            || {
                let end = u32::try_from(self.source.len()).unwrap_or(u32::MAX);
                Span::new(end, end)
            },
            |t| t.span,
        )
    }

    /// Span from token index `start` through the last consumed token.
    pub(crate) fn span_from(&self, start: usize) -> Span {
        let end = self.pos;
        let start_token = self.tokens.get(start).map(|t| t.span.start);
        let end_token = self.tokens.get(end.saturating_sub(1)).map(|t| t.span.end);
        match (start_token, end_token) {
            (Some(s), Some(e)) => Span::new(s, e),
            _ => self.current_span(),
        }
    }

    /// Builds [`ParseError::UnsupportedSyntax`] at the current location.
    pub(crate) fn reject_unsupported(&self, feature: &'static str) -> ParseError {
        ParseError::UnsupportedSyntax {
            feature,
            span: self.current_span(),
        }
    }

    /// Rejects post-MVP `@` / `#derive` directives at the current token.
    pub(crate) fn reject_deferred_directive(&self) -> ParseError {
        let feature = match self.peek_kind() {
            TokenKind::AtSpawn => "@spawn directive",
            TokenKind::AtSend => "@send directive",
            TokenKind::AtReceive => "@receive directive",
            TokenKind::AtReply => "@reply directive",
            TokenKind::HashDerive => "#derive directive",
            _ => "runtime directive",
        };
        self.reject_unsupported(feature)
    }

    /// Human-readable label for a token kind in diagnostics.
    ///
    /// Uses [`Cow::Borrowed`] for fixed phrases; allocates only when the message embeds a
    /// lexeme (`Ident`, `TypeIdent`, `Keyword`, etc.).
    pub(crate) fn found_description(kind: &TokenKind<'_>) -> Cow<'static, str> {
        match kind {
            TokenKind::Eof => Cow::Borrowed("end of file"),
            TokenKind::Keyword(k) => Cow::Owned(format!("keyword `{k:?}`")),
            TokenKind::Ident(s) => Cow::Owned(format!("identifier `{s}`")),
            TokenKind::TypeIdent(s) => Cow::Owned(format!("type identifier `{s}`")),
            TokenKind::Integer { .. } => Cow::Borrowed("integer literal"),
            TokenKind::Float { .. } => Cow::Borrowed("float literal"),
            TokenKind::Bool(b) => Cow::Owned(format!("boolean `{b}`")),
            TokenKind::ByteChar(_) => Cow::Borrowed("byte character literal"),
            TokenKind::ByteString(_) => Cow::Borrowed("byte string literal"),
            other => Cow::Owned(format!("{other:?}")),
        }
    }

    /// Builds an unexpected-token or unexpected-EOF error at the cursor.
    pub(crate) fn error_unexpected(&self, expected: ExpectedToken) -> ParseError {
        if self.at_end() {
            return ParseError::UnexpectedEof {
                expected,
                span: self.current_span(),
            };
        }
        let Some(token) = self.peek() else {
            return ParseError::UnexpectedEof {
                expected,
                span: self.current_span(),
            };
        };
        ParseError::UnexpectedToken {
            expected,
            found: Self::found_description(&token.kind),
            span: token.span,
        }
    }

    /// Consumes a token only if its kind equals `kind`.
    pub(crate) fn expect_kind(
        &mut self,
        expected: ExpectedToken,
        kind: &TokenKind<'src>,
    ) -> Result<Token<'src>, ParseError> {
        let token = self.peek().ok_or_else(|| ParseError::UnexpectedEof {
            expected,
            span: self.current_span(),
        })?;
        if token.kind == *kind {
            let Some(t) = self.bump() else {
                return Err(ParseError::UnexpectedEof {
                    expected,
                    span: self.current_span(),
                });
            };
            return Ok(t.clone());
        }
        Err(ParseError::UnexpectedToken {
            expected,
            found: Self::found_description(&token.kind),
            span: token.span,
        })
    }

    /// Parses a `snake_case` identifier.
    pub(crate) fn parse_ident(&mut self) -> Result<crate::ast::Ident, ParseError> {
        match self.peek_kind() {
            TokenKind::Ident(name) => {
                self.bump();
                Ok(self.intern_ident(name))
            }
            _ => Err(self.error_unexpected(ExpectedToken::Ident)),
        }
    }

    /// Parses a `PascalCase` type name (or keyword `Self`).
    pub(crate) fn parse_type_name(&mut self) -> Result<crate::ast::TypeName, ParseError> {
        match self.peek_kind() {
            TokenKind::TypeIdent(name) => {
                self.bump();
                Ok(self.intern_type_name(name))
            }
            TokenKind::Keyword(Keyword::SelfUpper) => {
                self.bump();
                Ok(self.intern_type_name("Self"))
            }
            _ => Err(self.error_unexpected(ExpectedToken::TypeIdent)),
        }
    }

    /// Requires a trailing `;`.
    pub(crate) fn expect_semi(&mut self) -> Result<(), ParseError> {
        if self.eat_kind(&TokenKind::Semicolon) {
            Ok(())
        } else {
            Err(self.error_unexpected(ExpectedToken::Punct(";")))
        }
    }

    /// Consumes the next token and returns its span (used for punct-specific recovery).
    #[allow(dead_code)]
    pub(crate) fn expect_punct(&mut self, punct: &'static str) -> Result<Span, ParseError> {
        let span = self.current_span();
        let token = self.bump().ok_or(ParseError::UnexpectedEof {
            expected: ExpectedToken::Punct(punct),
            span,
        })?;
        Ok(token.span)
    }

    /// Consumes `kw` when the next token is that keyword.
    pub(crate) fn eat_keyword(&mut self, kw: Keyword) -> bool {
        if matches!(self.peek_kind(), TokenKind::Keyword(k) if k == kw) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consumes the next token when its kind equals `kind`.
    pub(crate) fn eat_kind(&mut self, kind: &TokenKind<'src>) -> bool {
        if self.peek_kind() == *kind {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Parses imports then top-level items until EOF.
    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut imports = Vec::new();
        while matches!(self.peek_kind(), TokenKind::HashImport) {
            imports.push(self.parse_import()?);
        }
        let mut items = Vec::new();
        while !self.at_end() {
            if matches!(self.peek_kind(), TokenKind::HashDerive) {
                return Err(self.reject_unsupported("#derive directive"));
            }
            items.push(self.parse_top_level_item()?);
        }
        Ok(Program { imports, items })
    }
}

/// Parses `source` into a [`SourceFile`] (program + interner).
///
/// # Errors
///
/// Returns [`ParseError`] on lexical or syntactic failure.
pub fn parse(source: &str) -> Result<SourceFile, ParseError> {
    parse_with_interner(source, &mut Interner::new())
}

/// Parses `source` using `interner` for all identifiers (shared across a crate).
///
/// # Errors
///
/// Returns [`ParseError`] on lexical or syntactic failure.
pub fn parse_with_interner(
    source: &str,
    interner: &mut Interner,
) -> Result<SourceFile, ParseError> {
    let tokens = lex(source).map_err(ParseError::Lex)?;
    let mut parser = Parser::with_interner(source, &tokens, std::mem::take(interner));
    let program = parser.parse_program()?;
    *interner = parser.interner;
    Ok(SourceFile::new(program, interner.clone()))
}

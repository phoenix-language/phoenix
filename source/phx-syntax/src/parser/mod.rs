//! Recursive-descent parser for Phoenix source.

mod decl;
mod expr;
mod pat;
mod stmt;
mod types;

use phx_diagnostics::{ExpectedToken, ParseError, Span};

use crate::ast::Program;
use crate::intern::Interner;
use crate::lexer::lex;
use crate::token::{Keyword, Token, TokenKind};

/// Parser over a token stream and source text.
pub(crate) struct Parser<'src> {
    pub(crate) source: &'src str,
    pub(crate) tokens: &'src [Token<'src>],
    pub(crate) pos: usize,
    pub(crate) interner: Interner,
}

impl<'src> Parser<'src> {
    pub(crate) fn new(source: &'src str, tokens: &'src [Token<'src>]) -> Self {
        Self {
            source,
            tokens,
            pos: 0,
            interner: Interner::new(),
        }
    }

    pub(crate) fn intern_ident(&mut self, text: &str) -> crate::ast::Ident {
        crate::ast::Ident {
            symbol: self.interner.intern(text),
        }
    }

    pub(crate) fn intern_type_name(&mut self, text: &str) -> crate::ast::TypeName {
        crate::ast::TypeName {
            symbol: self.interner.intern(text),
        }
    }

    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.tokens.len() || matches!(self.peek_kind(), TokenKind::Eof)
    }

    pub(crate) fn peek(&self) -> Option<&Token<'src>> {
        self.tokens.get(self.pos)
    }

    pub(crate) fn peek_kind(&self) -> TokenKind<'src> {
        self.peek().map_or(TokenKind::Eof, |t| t.kind.clone())
    }

    /// Returns the token kind at `pos + n` without consuming.
    #[allow(dead_code)]
    pub(crate) fn peek_at(&self, n: usize) -> TokenKind<'src> {
        self.tokens
            .get(self.pos + n)
            .map_or(TokenKind::Eof, |t| t.kind.clone())
    }

    pub(crate) fn bump(&mut self) -> Option<&Token<'src>> {
        if self.at_end() {
            return None;
        }
        let t = &self.tokens[self.pos];
        self.pos += 1;
        Some(t)
    }

    pub(crate) fn current_span(&self) -> Span {
        self.peek().map_or_else(
            || {
                let end = u32::try_from(self.source.len()).unwrap_or(u32::MAX);
                Span::new(end, end)
            },
            |t| t.span,
        )
    }

    pub(crate) fn span_from(&self, start: usize) -> Span {
        let end = self.pos;
        let start_token = self.tokens.get(start).map(|t| t.span.start);
        let end_token = self.tokens.get(end.saturating_sub(1)).map(|t| t.span.end);
        match (start_token, end_token) {
            (Some(s), Some(e)) => Span::new(s, e),
            _ => self.current_span(),
        }
    }

    pub(crate) fn reject_unsupported(&self, feature: &'static str) -> ParseError {
        ParseError::UnsupportedSyntax {
            feature,
            span: self.current_span(),
        }
    }

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

    pub(crate) fn found_description(kind: &TokenKind<'_>) -> String {
        match kind {
            TokenKind::Eof => "end of file".to_owned(),
            TokenKind::Keyword(k) => format!("keyword `{k:?}`"),
            TokenKind::Ident(s) => format!("identifier `{s}`"),
            TokenKind::TypeIdent(s) => format!("type identifier `{s}`"),
            TokenKind::Integer { .. } => "integer literal".to_owned(),
            TokenKind::Float { .. } => "float literal".to_owned(),
            TokenKind::Bool(b) => format!("boolean `{b}`"),
            TokenKind::ByteChar(_) => "byte character literal".to_owned(),
            TokenKind::ByteString(_) => "byte string literal".to_owned(),
            other => format!("{other:?}"),
        }
    }

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

    pub(crate) fn parse_ident(&mut self) -> Result<crate::ast::Ident, ParseError> {
        match self.peek_kind() {
            TokenKind::Ident(name) => {
                self.bump();
                Ok(self.intern_ident(name))
            }
            _ => Err(self.error_unexpected(ExpectedToken::Ident)),
        }
    }

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

    pub(crate) fn eat_keyword(&mut self, kw: Keyword) -> bool {
        if matches!(self.peek_kind(), TokenKind::Keyword(k) if k == kw) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub(crate) fn eat_kind(&mut self, kind: &TokenKind<'src>) -> bool {
        if self.peek_kind() == *kind {
            self.bump();
            true
        } else {
            false
        }
    }

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

/// Parses `source` into a [`Program`] AST.
///
/// # Errors
///
/// Returns [`ParseError`] on lexical or syntactic failure.
pub fn parse(source: &str) -> Result<Program, ParseError> {
    let tokens = lex(source).map_err(ParseError::Lex)?;
    let mut parser = Parser::new(source, &tokens);
    parser.parse_program()
}

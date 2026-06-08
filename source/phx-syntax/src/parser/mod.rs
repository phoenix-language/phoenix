//! Recursive-descent parser for Phoenix source.
//!
//! The `Parser` holds the token cursor, source slice, and [`Interner`]. Submodules split the
//! grammar by syntactic category (`decl`, `expr`, `stmt`, `pat`, `types`). Entry point:
//! [`parse`].

mod attr;
mod decl;
mod expr;
mod pat;
mod stmt;
mod types;

use std::borrow::Cow;

use phx_diagnostics::{ExpectedToken, ParseBag, ParseError, Span};

use crate::ast::{AstNodeId, Node, Program};
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
    /// When set, parse errors are collected and parsing continues at sync points.
    recovery: Option<*mut ParseBag>,
    /// Next [`AstNodeId`] to assign (monotonic per parse).
    next_node_id: u32,
    /// Set when an inner `>` was absorbed from `>>` and closes the enclosing generic list.
    deferred_generic_closing: bool,
}

impl<'src> Parser<'src> {
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
            recovery: None,
            next_node_id: 0,
            deferred_generic_closing: false,
        }
    }

    /// Allocates the next AST node id for this parse.
    pub(crate) fn alloc_node_id(&mut self) -> AstNodeId {
        let id = self.next_node_id;
        self.next_node_id = id.saturating_add(1);
        AstNodeId::from_raw(id)
    }

    /// Wraps `inner` in a [`Node`] with `span` and a fresh id.
    pub(crate) fn node<T>(&mut self, inner: T, span: Span) -> Node<T> {
        Node::new(inner, span, self.alloc_node_id())
    }

    /// Enables error recovery into `bag` for the remainder of this parse.
    pub(crate) fn enable_recovery(&mut self, bag: &mut ParseBag) {
        self.recovery = Some(std::ptr::from_mut(bag));
    }

    fn recovery_bag(&mut self) -> Option<&mut ParseBag> {
        self.recovery.map(|ptr| {
            // SAFETY: `enable_recovery` sets this from `&mut ParseBag` on the same parser instance.
            unsafe { &mut *ptr }
        })
    }

    fn record_error(&mut self, error: ParseError) {
        if let Some(bag) = self.recovery_bag() {
            bag.push(error);
        }
    }

    fn in_recovery_mode(&self) -> bool {
        self.recovery.is_some()
    }

    /// Advances until `;`, `}`, or EOF (always consumes at least one token).
    pub(crate) fn sync_stmt(&mut self) {
        let start = self.pos;
        while !self.at_end() {
            match self.peek_kind() {
                TokenKind::Semicolon => {
                    let _ = self.bump();
                    return;
                }
                TokenKind::RBrace | TokenKind::Eof => return,
                _ => {
                    let _ = self.bump();
                }
            }
        }
        if self.pos == start && !self.at_end() {
            let _ = self.bump();
        }
    }

    /// Advances until a plausible top-level item start, `;`, `}`, or EOF.
    pub(crate) fn sync_top_level(&mut self) {
        let start = self.pos;
        while !self.at_end() {
            match self.peek_kind() {
                TokenKind::Semicolon => {
                    let _ = self.bump();
                    return;
                }
                TokenKind::RBrace
                | TokenKind::HashBracket
                | TokenKind::HashImport
                | TokenKind::HashDerive
                | TokenKind::HashInline
                | TokenKind::HashCold
                | TokenKind::HashHot
                | TokenKind::HashUnsafe
                | TokenKind::Ident(_)
                | TokenKind::TypeIdent(_)
                | TokenKind::Keyword(
                    Keyword::Pub | Keyword::Type | Keyword::Const | Keyword::Var,
                ) => {
                    return;
                }
                _ => {
                    let _ = self.bump();
                }
            }
        }
        if self.pos == start && !self.at_end() {
            let _ = self.bump();
        }
    }

    /// Interns `text` as a value identifier (`snake_case` name).
    /// Consumes the current identifier token and interns it.
    pub(crate) fn bump_ident(&mut self, text: &str) -> Result<crate::ast::Ident, ParseError> {
        let span = self.current_span();
        self.bump();
        self.intern_ident(text, span)
    }

    /// Interns `text` at `span` as a value [`Ident`].
    pub(crate) fn intern_ident(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<crate::ast::Ident, ParseError> {
        let symbol = self
            .interner
            .intern(text)
            .map_err(|_| ParseError::InternTableFull { span })?;
        Ok(crate::ast::Ident {
            symbol,
            span,
            id: self.alloc_node_id(),
        })
    }

    /// Interns `text` as a type identifier (`PascalCase` name) at `span`.
    pub(crate) fn intern_type_name(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<crate::ast::TypeName, ParseError> {
        let symbol = self
            .interner
            .intern(text)
            .map_err(|_| ParseError::InternTableFull { span })?;
        Ok(crate::ast::TypeName {
            symbol,
            span,
            id: self.alloc_node_id(),
        })
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
            TokenKind::String(_) => Cow::Borrowed("string literal"),
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
                let span = self.current_span();
                self.bump();
                self.intern_ident(name, span)
            }
            _ => Err(self.error_unexpected(ExpectedToken::Ident)),
        }
    }

    /// Parses a `PascalCase` type name (or keyword `Self`).
    pub(crate) fn parse_type_name(&mut self) -> Result<crate::ast::TypeName, ParseError> {
        match self.peek_kind() {
            TokenKind::TypeIdent(name) => {
                let span = self.current_span();
                self.bump();
                self.intern_type_name(name, span)
            }
            TokenKind::Keyword(Keyword::SelfUpper) => {
                let span = self.current_span();
                self.bump();
                self.intern_type_name("Self", span)
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
            match self.parse_import() {
                Ok(imp) => imports.push(imp),
                Err(e) => {
                    if self.in_recovery_mode() {
                        self.record_error(e);
                        self.sync_top_level();
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        let mut items = Vec::new();
        while !self.at_end() {
            let at_start = self.pos;
            match self.parse_top_level_item() {
                Ok(item) => items.push(item),
                Err(e) => {
                    if self.in_recovery_mode() {
                        self.record_error(e);
                        self.sync_top_level();
                        if self.at_end() || self.pos == at_start {
                            break;
                        }
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        Ok(Program { imports, items })
    }
}

/// Parses `source` into a [`SourceFile`] (program + interner).
///
/// # Errors
///
/// Returns [`ParseBag`] when lexical or syntactic errors were collected.
pub fn parse(source: &str) -> Result<SourceFile, ParseBag> {
    parse_with_interner(source, &mut Interner::new())
}

/// Parses `source` using `interner` for all identifiers (shared across a crate).
///
/// # Errors
///
/// Returns [`ParseBag`] when any parse errors were collected.
pub fn parse_with_interner(source: &str, interner: &mut Interner) -> Result<SourceFile, ParseBag> {
    let tokens = match lex(source) {
        Ok(t) => t,
        Err(e) => return Err(ParseBag::from_single(ParseError::Lex(e))),
    };
    let mut bag = ParseBag::new();
    let mut parser = Parser::with_interner(source, &tokens, std::mem::take(interner));
    parser.enable_recovery(&mut bag);
    let program = match parser.parse_program() {
        Ok(p) => p,
        Err(e) => {
            bag.push(e);
            return Err(bag);
        }
    };
    *interner = parser.interner;
    if bag.has_errors() {
        return Err(bag);
    }
    Ok(SourceFile::new(program, interner.clone()))
}

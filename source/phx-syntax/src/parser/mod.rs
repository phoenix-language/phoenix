//! Recursive-descent parser for Phoenix source.
//!
//! ## Public entry points
//!
//! Call [`parse`] for a single file, or [`parse_with_interner`] when building a crate and
//! reusing one [`Interner`] across multiple compilation units. Both functions:
//!
//! 1. Lex the source via [`crate::lexer::lex`].
//! 2. Run the internal [`Parser`] over the token stream.
//! 3. Return a [`ParseResult`] containing a [`SourceFile`] (program + interner) and any
//!    collected diagnostics.
//!
//! ## Grammar layout
//!
//! The internal [`Parser`] holds the token cursor, source slice, and [`Interner`]. Submodules
//! split the grammar by syntactic category:
//!
//! | Submodule | Parses |
//! |-----------|--------|
//! | `decl` | imports, functions, types, impls, modules |
//! | `expr` | expressions and literals |
//! | `stmt` | statements and blocks |
//! | `pat` | patterns |
//! | `types` | type expressions |
//! | `attr` | `#[…]` item attributes |
//!
//! ## Error recovery
//!
//! [`parse`] and [`parse_with_interner`] always enable recovery mode. On a syntax error the
//! parser records a [`ParseError`] into a [`ParseBag`] and resumes at a **sync point** instead
//! of aborting the file:
//!
//! - **Top-level items** (imports, declarations): [`Parser::sync_top_level`] skips tokens until
//!   the next plausible item start (`;`, `}`, attribute, keyword, or identifier).
//! - **Statements** (inside blocks): [`Parser::sync_stmt`] skips until `;`, `}`, or EOF.
//!
//! ### Invariants
//!
//! - **Never panics on user input.** Malformed source, lexer failures, and exhausted intern
//!   tables produce [`ParseError`] values; they do not trigger Rust panics.
//! - **Always returns a value.** Even when lexing fails or every top-level item errors, callers
//!   receive a [`ParseResult`] with an (possibly empty) [`Program`] and populated `errors`.
//! - **Recovery is best-effort.** Sync points advance the cursor; if progress stalls (cursor
//!   unchanged at EOF), the top-level loop stops to avoid an infinite skip.
//! - **Partial AST is valid for downstream passes.** Later compiler stages should inspect
//!   [`ParseResult::has_errors`] and/or merge `errors` before assuming a complete tree.
//! - **Spans refer to the original `source` string.** The returned [`SourceFile`] does not
//!   own source text; offsets in diagnostics and AST nodes index into the `&str` argument.

mod attr;
mod decl;
mod expr;
mod pat;
mod stmt;
mod types;

use std::borrow::Cow;

use phx_diagnostics::{ExpectedToken, ParseBag, ParseError, ParseResult, Span};

use crate::ast::{AstNodeId, Node, Program};
use crate::intern::Interner;
use crate::lexer::lex;
use crate::source_file::SourceFile;
use crate::token::{Keyword, Token, TokenKind};

/// Sentinel returned by [`Parser::peek_kind`] / [`Parser::peek_at`] past the token stream.
const EOF_KIND: TokenKind<'static> = TokenKind::Eof;

/// Mutable parse state over a lexed token stream and source text.
///
/// Callers outside this module use [`parse`] / [`parse_with_interner`]; submodule impl blocks
/// extend `Parser` with category-specific `parse_*` methods. The cursor (`pos`) only moves
/// forward via [`Parser::bump`] except when speculative parsing restores a [`Parser::checkpoint`].
pub(crate) struct Parser<'src> {
    /// Original source buffer (for spans and literal text).
    source: &'src str,
    /// Token stream from [`crate::lexer::lex`].
    tokens: &'src [Token<'src>],
    /// Index of the current token in `tokens`.
    pos: usize,
    /// Intern table filled while parsing identifiers.
    interner: Interner,
    /// When set, parse errors are collected and parsing continues at sync points.
    recovery: Option<ParseBag>,
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

    /// Returns the current token index (for span bookkeeping).
    pub(crate) fn checkpoint(&self) -> usize {
        self.pos
    }

    /// Restores the token cursor to a prior [`Self::checkpoint`].
    pub(crate) fn restore(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Returns the intern table built during this parse.
    pub(crate) fn into_interner(self) -> Interner {
        self.interner
    }

    /// Enables error recovery for the remainder of this parse.
    pub(crate) fn enable_recovery(&mut self) {
        self.recovery = Some(ParseBag::new());
    }

    /// Takes the collected recovery errors, leaving recovery disabled.
    pub(crate) fn take_recovery_bag(&mut self) -> ParseBag {
        self.recovery.take().unwrap_or_default()
    }

    fn record_error(&mut self, error: ParseError) {
        if let Some(bag) = &mut self.recovery {
            bag.push(error);
        }
    }

    fn in_recovery_mode(&self) -> bool {
        self.recovery.is_some()
    }

    /// Advances until `;`, `}`, or EOF (always consumes at least one token).
    pub(crate) fn sync_stmt(&mut self) {
        let start = self.checkpoint();
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
        if self.checkpoint() == start && !self.at_end() {
            let _ = self.bump();
        }
    }

    /// Advances until a plausible top-level item start, `;`, `}`, or EOF.
    pub(crate) fn sync_top_level(&mut self) {
        let start = self.checkpoint();
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
                | TokenKind::Ident(_)
                | TokenKind::TypeIdent(_)
                | TokenKind::Keyword(
                    Keyword::Unsafe
                    | Keyword::Extern
                    | Keyword::Pub
                    | Keyword::Mod
                    | Keyword::Reexport
                    | Keyword::Type
                    | Keyword::Const
                    | Keyword::Var,
                ) => {
                    return;
                }
                _ => {
                    let _ = self.bump();
                }
            }
        }
        if self.checkpoint() == start && !self.at_end() {
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
    pub(crate) fn peek_kind(&self) -> &TokenKind<'src> {
        self.peek().map_or(&EOF_KIND, |t| &t.kind)
    }

    /// Returns the token kind at `pos + n` without consuming.
    pub(crate) fn peek_at(&self, n: usize) -> &TokenKind<'src> {
        self.tokens.get(self.pos + n).map_or(&EOF_KIND, |t| &t.kind)
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

    /// Advances and returns the consumed token kind (single clone).
    pub(crate) fn bump_kind(&mut self) -> Option<TokenKind<'src>> {
        let idx = self.pos;
        self.bump()?;
        Some(self.tokens[idx].kind.clone())
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
        let name = match self.peek_kind() {
            TokenKind::Ident(name) => *name,
            _ => return Err(self.error_unexpected(ExpectedToken::Ident)),
        };
        let span = self.current_span();
        self.bump();
        self.intern_ident(name, span)
    }

    /// Parses a tuple struct field name: `ident` or integer index (`0`, `1`, …).
    pub(crate) fn parse_tuple_field_name(&mut self) -> Result<crate::ast::Ident, ParseError> {
        match self.peek_kind() {
            TokenKind::Ident(name) => {
                let name = *name;
                let span = self.current_span();
                self.bump();
                self.intern_ident(name, span)
            }
            TokenKind::Integer { value, .. } if *value >= 0 => {
                let value = *value;
                let span = self.current_span();
                self.bump();
                let name = value.to_string();
                self.intern_ident(&name, span)
            }
            _ => Err(self.error_unexpected(ExpectedToken::Ident)),
        }
    }

    /// Parses one symbol in a braced `#import` list (`foo` or `Error`).
    pub(crate) fn parse_import_symbol(&mut self) -> Result<crate::ast::Ident, ParseError> {
        let name = match self.peek_kind() {
            TokenKind::Ident(name) | TokenKind::TypeIdent(name) => *name,
            _ => return Err(self.error_unexpected(ExpectedToken::Ident)),
        };
        let span = self.current_span();
        self.bump();
        self.intern_ident(name, span)
    }

    /// Parses a type alias name (`PascalCase` or lowercase C-style `c_int`).
    pub(crate) fn parse_type_alias_name(&mut self) -> Result<crate::ast::TypeName, ParseError> {
        match self.peek_kind() {
            TokenKind::TypeIdent(name) | TokenKind::Ident(name) => {
                let name = *name;
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

    /// Parses a `PascalCase` type name (or keyword `Self`, or primitive type keywords in impl blocks).
    pub(crate) fn parse_type_name(&mut self) -> Result<crate::ast::TypeName, ParseError> {
        match self.peek_kind() {
            TokenKind::TypeIdent(name) => {
                let name = *name;
                let span = self.current_span();
                self.bump();
                self.intern_type_name(name, span)
            }
            TokenKind::Keyword(Keyword::SelfUpper) => {
                let span = self.current_span();
                self.bump();
                self.intern_type_name("Self", span)
            }
            TokenKind::Keyword(kw) if is_primitive_type_keyword(*kw) => {
                let span = self.current_span();
                let name = primitive_type_keyword_name(*kw);
                self.bump();
                self.intern_type_name(name, span)
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
        if matches!(self.peek_kind(), TokenKind::Keyword(k) if *k == kw) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Returns true when the next token is `kw`.
    pub(crate) fn peek_keyword(&self, kw: Keyword) -> bool {
        matches!(self.peek_kind(), TokenKind::Keyword(k) if *k == kw)
    }

    /// Requires the next token to be `kw`.
    pub(crate) fn expect_keyword(&mut self, kw: Keyword) -> Result<(), ParseError> {
        if self.eat_keyword(kw) {
            Ok(())
        } else {
            let label = match kw {
                Keyword::Mod => "mod",
                Keyword::Reexport => "reexport",
                _ => "keyword",
            };
            Err(self.error_unexpected(ExpectedToken::Punct(label)))
        }
    }

    /// Consumes the next token when its kind equals `kind`.
    pub(crate) fn eat_kind(&mut self, kind: &TokenKind<'src>) -> bool {
        if self.peek_kind() == kind {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Parses `#import` directives then top-level items until EOF.
    ///
    /// With recovery enabled, import and item errors are recorded and parsing continues at
    /// [`Self::sync_top_level`]; without recovery, the first error aborts the parse.
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
            let at_start = self.checkpoint();
            match self.parse_top_level_item() {
                Ok(item) => items.push(item),
                Err(e) => {
                    if self.in_recovery_mode() {
                        self.record_error(e);
                        self.sync_top_level();
                        if self.at_end() || self.checkpoint() == at_start {
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

/// Parses a single Phoenix source file into a [`SourceFile`].
///
/// Convenience wrapper around [`parse_with_interner`] that allocates a fresh [`Interner`].
/// Use this for one-off parses (tests, REPL, single-file tools). For multi-file crates,
/// prefer [`parse_with_interner`] so identifier symbols are shared across units.
///
/// # Recovery
///
/// Lex and parse errors are collected; parsing continues at sync points (see module docs).
/// Check [`ParseResult::has_errors`] before treating the AST as complete.
///
/// # Panics
///
/// Never panics on malformed user input.
///
/// # Examples
///
/// ```
/// use phx_syntax::parse;
///
/// let result = parse("main :: () => { };");
/// assert!(!result.has_errors());
/// assert_eq!(result.value.program.items.len(), 1);
/// ```
#[must_use]
pub fn parse(source: &str) -> ParseResult<SourceFile> {
    parse_with_interner(source, &mut Interner::new())
}

fn is_primitive_type_keyword(kw: Keyword) -> bool {
    matches!(
        kw,
        Keyword::Bool
            | Keyword::S8
            | Keyword::S16
            | Keyword::S32
            | Keyword::S64
            | Keyword::U8
            | Keyword::U16
            | Keyword::U32
            | Keyword::U64
            | Keyword::F32
            | Keyword::F64
            | Keyword::Str
    )
}

fn primitive_type_keyword_name(kw: Keyword) -> &'static str {
    match kw {
        Keyword::Bool => "bool",
        Keyword::S8 => "s8",
        Keyword::S16 => "s16",
        Keyword::S32 => "s32",
        Keyword::S64 => "s64",
        Keyword::U8 => "u8",
        Keyword::U16 => "u16",
        Keyword::U32 => "u32",
        Keyword::U64 => "u64",
        Keyword::F32 => "f32",
        Keyword::F64 => "f64",
        Keyword::Str => "str",
        _ => "",
    }
}

/// Parses `source`, interning identifiers into `interner`.
///
/// Takes ownership of the interner's current contents via [`std::mem::take`], parses, then
/// writes the updated table back into `interner`. Callers building a crate can pass the same
/// `interner` for every file so [`Symbol`](crate::Symbol) ids are stable across compilation units.
///
/// # Recovery
///
/// Recovery mode is always enabled. Behavior on failure:
///
/// | Stage | On error |
/// |-------|----------|
/// | Lex | Returns an empty [`Program`] and a single [`ParseError::Lex`]; no parse pass runs. |
/// | Parse (top-level) | Records the error, syncs to the next item, continues until EOF. |
/// | Parse (fatal in non-recovery paths) | Recorded and replaced with an empty program (should not occur via this entry point). |
///
/// The returned [`ParseResult::value`] always contains a [`SourceFile`] whose `program` may be
/// partial when `errors` is non-empty.
///
/// # Panics
///
/// Never panics on malformed user input.
#[must_use]
pub fn parse_with_interner(source: &str, interner: &mut Interner) -> ParseResult<SourceFile> {
    let empty_program = Program {
        imports: Vec::new(),
        items: Vec::new(),
    };
    let tokens = match lex(source) {
        Ok(t) => t,
        Err(e) => {
            return ParseResult::with_errors(
                SourceFile::new(empty_program, interner.clone()),
                vec![ParseError::Lex(e)],
            );
        }
    };
    let mut parser = Parser::with_interner(source, &tokens, std::mem::take(interner));
    parser.enable_recovery();
    let program = match parser.parse_program() {
        Ok(p) => p,
        Err(e) => {
            parser.record_error(e);
            empty_program
        }
    };
    let bag = parser.take_recovery_bag();
    *interner = parser.into_interner();
    ParseResult::with_errors(
        SourceFile::new(program, interner.clone()),
        bag.into_errors(),
    )
}

//! Phoenix lexer — source text to token stream.
//!
//! Tokens borrow lexeme text from the source (`Token<'src>`). Numeric lexemes are parsed
//! in-place (underscore separators are skipped without allocation); byte string payloads use
//! `Vec<u8>`.

use phx_diagnostics::{LexError, Span};

use crate::token::{FloatSuffix, IntegerSuffix, Keyword, Token, TokenKind};

/// Maximum length of a single lexeme in bytes.
const MAX_LEXEME_LEN: usize = 1_048_576;

/// Lexical analyzer over a borrowed source buffer.
#[derive(Debug)]
pub struct Lexer<'src> {
    source: &'src str,
    cursor: usize,
    start: usize,
}

impl<'src> Lexer<'src> {
    /// Creates a lexer positioned at the start of `source`.
    #[must_use]
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            cursor: 0,
            start: 0,
        }
    }

    /// Returns the next token, or an error.
    ///
    /// # Errors
    ///
    /// Returns [`LexError`] on malformed input.
    pub fn next_token(&mut self) -> Result<Token<'src>, LexError> {
        self.skip_trivia()?;
        self.start = self.cursor;
        if self.is_at_end() {
            return Ok(self.make_token(TokenKind::Eof));
        }
        let b = self.peek_byte().ok_or_else(|| self.unexpected_eof())?;
        let kind = match b {
            b',' => {
                self.advance();
                TokenKind::Comma
            }
            b';' => {
                self.advance();
                TokenKind::Semicolon
            }
            b'.' => self.lex_dot(),
            b':' => self.lex_colon(),
            b'(' => {
                self.advance();
                TokenKind::LParen
            }
            b')' => {
                self.advance();
                TokenKind::RParen
            }
            b'{' => {
                self.advance();
                TokenKind::LBrace
            }
            b'}' => {
                self.advance();
                TokenKind::RBrace
            }
            b'[' => {
                self.advance();
                TokenKind::LBracket
            }
            b']' => {
                self.advance();
                TokenKind::RBracket
            }
            b'+' => self.lex_plus(),
            b'-' => self.lex_minus(),
            b'*' => self.lex_star(),
            b'/' => self.lex_slash(),
            b'%' => self.lex_percent(),
            b'&' => self.lex_amp(),
            b'|' => self.lex_pipe(),
            b'^' => self.lex_caret(),
            b'~' => {
                self.advance();
                TokenKind::Tilde
            }
            b'!' => self.lex_bang(),
            b'=' => self.lex_eq(),
            b'<' => self.lex_lt(),
            b'>' => self.lex_gt(),
            b'?' => {
                self.advance();
                TokenKind::Question
            }
            b'#' => self.lex_hash_directive()?,
            b'@' => self.lex_at_directive()?,
            b'"' => self.lex_string_literal()?,
            b'b' => {
                if self.peek_byte_at(1) == Some(b'\'') || self.peek_byte_at(1) == Some(b'"') {
                    self.lex_byte_literal()?
                } else {
                    self.lex_number_or_ident()?
                }
            }
            b'0'..=b'9' => self.lex_number()?,
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.lex_ident()?,
            _ => {
                let ch = self.current_char().ok_or_else(|| self.unexpected_eof())?;
                return Err(LexError::UnexpectedChar {
                    ch,
                    offset: u32::try_from(self.cursor).unwrap_or(u32::MAX),
                });
            }
        };
        Ok(self.make_token(kind))
    }

    fn lex_dot(&mut self) -> TokenKind<'src> {
        self.advance();
        if self.consume_byte(b'.') {
            if self.consume_byte(b'=') {
                TokenKind::DotDotEq
            } else {
                TokenKind::DotDot
            }
        } else {
            TokenKind::Dot
        }
    }

    fn lex_colon(&mut self) -> TokenKind<'src> {
        self.advance();
        if self.consume_byte(b':') {
            TokenKind::ColonColon
        } else {
            TokenKind::Colon
        }
    }

    fn lex_plus(&mut self) -> TokenKind<'src> {
        self.advance();
        if self.consume_byte(b'=') {
            TokenKind::PlusEq
        } else {
            TokenKind::Plus
        }
    }

    fn lex_minus(&mut self) -> TokenKind<'src> {
        self.advance();
        if self.consume_byte(b'=') {
            TokenKind::MinusEq
        } else {
            TokenKind::Minus
        }
    }

    fn lex_star(&mut self) -> TokenKind<'src> {
        self.advance();
        if self.consume_byte(b'*') {
            TokenKind::StarStar
        } else if self.consume_byte(b'=') {
            TokenKind::StarEq
        } else if self.consume_bytes(b"mut") {
            TokenKind::StarMut
        } else {
            TokenKind::Star
        }
    }

    fn lex_slash(&mut self) -> TokenKind<'src> {
        self.advance();
        if self.consume_byte(b'=') {
            TokenKind::SlashEq
        } else {
            TokenKind::Slash
        }
    }

    fn lex_percent(&mut self) -> TokenKind<'src> {
        self.advance();
        if self.consume_byte(b'=') {
            TokenKind::PercentEq
        } else {
            TokenKind::Percent
        }
    }

    fn lex_amp(&mut self) -> TokenKind<'src> {
        self.advance();
        if self.consume_byte(b'&') {
            TokenKind::AndAnd
        } else if self.consume_bytes(b"mut") {
            TokenKind::AmpMut
        } else {
            TokenKind::Amp
        }
    }

    fn lex_pipe(&mut self) -> TokenKind<'src> {
        self.advance();
        if self.consume_byte(b'|') {
            TokenKind::OrOr
        } else {
            TokenKind::Pipe
        }
    }

    fn lex_caret(&mut self) -> TokenKind<'src> {
        self.advance();
        TokenKind::Caret
    }

    fn lex_bang(&mut self) -> TokenKind<'src> {
        self.advance();
        if self.consume_byte(b'=') {
            TokenKind::Ne
        } else {
            TokenKind::Bang
        }
    }

    fn lex_eq(&mut self) -> TokenKind<'src> {
        self.advance();
        if self.consume_byte(b'=') {
            TokenKind::EqEq
        } else if self.consume_byte(b'>') {
            TokenKind::FatArrow
        } else {
            TokenKind::Eq
        }
    }

    fn lex_lt(&mut self) -> TokenKind<'src> {
        self.advance();
        if self.consume_byte(b'=') {
            TokenKind::Le
        } else if self.consume_byte(b'<') {
            TokenKind::Shl
        } else {
            TokenKind::Lt
        }
    }

    fn lex_gt(&mut self) -> TokenKind<'src> {
        self.advance();
        if self.consume_byte(b'=') {
            TokenKind::Ge
        } else if self.consume_byte(b'>') {
            TokenKind::Shr
        } else {
            TokenKind::Gt
        }
    }

    fn lex_hash_directive(&mut self) -> Result<TokenKind<'src>, LexError> {
        self.advance();
        if self.consume_byte(b'[') {
            return Ok(TokenKind::HashBracket);
        }
        if self.consume_bytes(b"import") {
            Ok(TokenKind::HashImport)
        } else if self.consume_bytes(b"inline") {
            Ok(TokenKind::HashInline)
        } else if self.consume_bytes(b"cold") {
            Ok(TokenKind::HashCold)
        } else if self.consume_bytes(b"hot") {
            Ok(TokenKind::HashHot)
        } else if self.consume_bytes(b"derive") {
            Ok(TokenKind::HashDerive)
        } else {
            let ch = self.current_char().ok_or_else(|| self.unexpected_eof())?;
            Err(LexError::UnexpectedChar {
                ch,
                offset: u32::try_from(self.start).unwrap_or(u32::MAX),
            })
        }
    }

    fn lex_at_directive(&mut self) -> Result<TokenKind<'src>, LexError> {
        self.advance();
        if self.consume_bytes(b"spawn") {
            Ok(TokenKind::AtSpawn)
        } else if self.consume_bytes(b"send") {
            Ok(TokenKind::AtSend)
        } else if self.consume_bytes(b"receive") {
            Ok(TokenKind::AtReceive)
        } else if self.consume_bytes(b"reply") {
            Ok(TokenKind::AtReply)
        } else {
            let ch = self.current_char().ok_or_else(|| self.unexpected_eof())?;
            Err(LexError::UnexpectedChar {
                ch,
                offset: u32::try_from(self.start).unwrap_or(u32::MAX),
            })
        }
    }

    fn lex_number_or_ident(&mut self) -> Result<TokenKind<'src>, LexError> {
        if self.peek_byte_at(1).is_some_and(|b| b.is_ascii_digit()) {
            self.lex_number()
        } else {
            self.lex_ident()
        }
    }

    fn lex_number(&mut self) -> Result<TokenKind<'src>, LexError> {
        if self.peek_byte() == Some(b'0') {
            match self.peek_byte_at(1) {
                Some(b'x' | b'X') => return self.lex_hex_integer(),
                Some(b'b' | b'B') => return self.lex_binary_integer(),
                _ => {}
            }
        }
        if self.try_lex_float()? {
            return self.finish_float();
        }
        self.lex_decimal_integer()
    }

    fn lex_hex_integer(&mut self) -> Result<TokenKind<'src>, LexError> {
        self.advance();
        self.advance();
        if !self.peek_byte().is_some_and(is_hex_digit) {
            return Err(self.invalid_int());
        }
        while self
            .peek_byte()
            .is_some_and(|b| is_hex_digit(b) || b == b'_')
        {
            self.advance();
        }
        self.finish_integer(IntegerBase::Hex)
    }

    fn lex_binary_integer(&mut self) -> Result<TokenKind<'src>, LexError> {
        self.advance();
        self.advance();
        if !self.peek_byte().is_some_and(|b| b == b'0' || b == b'1') {
            return Err(self.invalid_int());
        }
        while self
            .peek_byte()
            .is_some_and(|b| b == b'0' || b == b'1' || b == b'_')
        {
            self.advance();
        }
        self.finish_integer(IntegerBase::Binary)
    }

    fn lex_decimal_integer(&mut self) -> Result<TokenKind<'src>, LexError> {
        while self
            .peek_byte()
            .is_some_and(|b| b.is_ascii_digit() || b == b'_')
        {
            self.advance();
        }
        self.finish_integer(IntegerBase::Decimal)
    }

    fn try_lex_float(&mut self) -> Result<bool, LexError> {
        let saved = self.cursor;
        while self
            .peek_byte()
            .is_some_and(|b| b.is_ascii_digit() || b == b'_')
        {
            self.advance();
        }
        let after_digits = self.cursor;
        if self.peek_byte() == Some(b'.')
            && self.peek_byte_at(1).is_some_and(|b| b.is_ascii_digit())
        {
            self.advance();
            while self
                .peek_byte()
                .is_some_and(|b| b.is_ascii_digit() || b == b'_')
            {
                self.advance();
            }
            if self.peek_byte().is_some_and(|b| b == b'e' || b == b'E') {
                self.lex_exponent_part()?;
            }
            return Ok(true);
        }
        if after_digits > saved && self.peek_byte().is_some_and(|b| b == b'e' || b == b'E') {
            self.lex_exponent_part()?;
            return Ok(true);
        }
        self.cursor = saved;
        Ok(false)
    }

    fn lex_exponent_part(&mut self) -> Result<(), LexError> {
        self.advance();
        if self.peek_byte().is_some_and(|b| b == b'+' || b == b'-') {
            self.advance();
        }
        if !self.peek_byte().is_some_and(|b| b.is_ascii_digit()) {
            return Err(self.invalid_float());
        }
        while self
            .peek_byte()
            .is_some_and(|b| b.is_ascii_digit() || b == b'_')
        {
            self.advance();
        }
        Ok(())
    }

    fn finish_float(&mut self) -> Result<TokenKind<'src>, LexError> {
        self.check_lexeme_len()?;
        let suffix = if self.consume_bytes(b"f64") {
            FloatSuffix::F64
        } else if self.consume_bytes(b"f32") {
            FloatSuffix::F32
        } else {
            FloatSuffix::None
        };
        let lexeme = self.current_lexeme();
        let numeric = match suffix {
            FloatSuffix::F64 => lexeme.strip_suffix("f64").unwrap_or(lexeme),
            FloatSuffix::F32 => lexeme.strip_suffix("f32").unwrap_or(lexeme),
            FloatSuffix::None => lexeme,
        };
        let value = parse_float_literal(numeric).map_err(|()| self.invalid_float())?;
        Ok(TokenKind::Float { value, suffix })
    }

    fn finish_integer(&mut self, base: IntegerBase) -> Result<TokenKind<'src>, LexError> {
        self.check_lexeme_len()?;
        let mut suffix = IntegerSuffix::None;
        if self.peek_byte() == Some(b'u') {
            suffix = IntegerSuffix::Unsigned;
            self.advance();
        }
        let lexeme = self.current_lexeme();
        let numeric_lexeme = if suffix == IntegerSuffix::Unsigned {
            lexeme.strip_suffix('u').unwrap_or(lexeme)
        } else {
            lexeme
        };
        if numeric_lexeme.is_empty() {
            return Err(self.invalid_int());
        }
        let value = match base {
            IntegerBase::Decimal => parse_decimal_integer(numeric_lexeme),
            IntegerBase::Hex => {
                let digits = numeric_lexeme
                    .strip_prefix("0x")
                    .or_else(|| numeric_lexeme.strip_prefix("0X"))
                    .unwrap_or(numeric_lexeme);
                parse_radix_integer(digits, 16, hex_digit_value)
            }
            IntegerBase::Binary => {
                let digits = numeric_lexeme
                    .strip_prefix("0b")
                    .or_else(|| numeric_lexeme.strip_prefix("0B"))
                    .unwrap_or(numeric_lexeme);
                parse_radix_integer(digits, 2, binary_digit_value)
            }
        }
        .map_err(|()| LexError::IntegerOverflow {
            start: self.span_start(),
            end: self.span_end(),
        })?;
        Ok(TokenKind::Integer { value, suffix })
    }

    fn lex_ident(&mut self) -> Result<TokenKind<'src>, LexError> {
        let first = self.peek_byte().ok_or_else(|| self.unexpected_eof())?;
        if is_ascii_upper(first) {
            self.lex_pascal_ident()
        } else {
            self.lex_snake_ident()
        }
    }

    fn lex_snake_ident(&mut self) -> Result<TokenKind<'src>, LexError> {
        if !self.peek_byte().is_some_and(is_snake_start) {
            return Err(self.unexpected_char());
        }
        self.advance();
        while self.peek_byte().is_some_and(is_snake_continue) {
            self.advance();
        }
        self.check_lexeme_len()?;
        let lexeme = self.current_lexeme();
        if lexeme == "true" {
            return Ok(TokenKind::Bool(true));
        }
        if lexeme == "false" {
            return Ok(TokenKind::Bool(false));
        }
        if let Some(kw) = Keyword::lookup(lexeme) {
            return Ok(TokenKind::Keyword(kw));
        }
        Ok(TokenKind::Ident(lexeme))
    }

    fn lex_pascal_ident(&mut self) -> Result<TokenKind<'src>, LexError> {
        if !self.peek_byte().is_some_and(is_ascii_upper) {
            return Err(self.unexpected_char());
        }
        self.advance();
        while self.peek_byte().is_some_and(|b| b.is_ascii_alphanumeric()) {
            self.advance();
        }
        self.check_lexeme_len()?;
        let lexeme = self.current_lexeme();
        if let Some(kw) = Keyword::lookup(lexeme) {
            return Ok(TokenKind::Keyword(kw));
        }
        Ok(TokenKind::TypeIdent(lexeme))
    }

    fn lex_byte_literal(&mut self) -> Result<TokenKind<'src>, LexError> {
        self.advance();
        if self.consume_byte(b'\'') {
            let value = self.read_byte_char_content()?;
            if !self.consume_byte(b'\'') {
                return Err(LexError::UnterminatedString {
                    start: self.span_start(),
                });
            }
            Ok(TokenKind::ByteChar(value))
        } else if self.consume_byte(b'"') {
            let mut bytes = Vec::new();
            while !self.is_at_end() && self.peek_byte() != Some(b'"') {
                bytes.push(self.read_byte_string_char()?);
            }
            if !self.consume_byte(b'"') {
                return Err(LexError::UnterminatedString {
                    start: self.span_start(),
                });
            }
            Ok(TokenKind::ByteString(bytes))
        } else {
            Err(self.unexpected_char())
        }
    }

    fn lex_string_literal(&mut self) -> Result<TokenKind<'src>, LexError> {
        let lit_start = self.span_start();
        self.advance();
        let mut bytes = Vec::new();
        while !self.is_at_end() && self.peek_byte() != Some(b'"') {
            if self.peek_byte() == Some(b'\\') {
                bytes.push(self.read_escape()?);
            } else {
                let ch = self
                    .next_char()?
                    .ok_or(LexError::UnterminatedString { start: lit_start })?;
                let mut buf = [0u8; 4];
                bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
        }
        if !self.consume_byte(b'"') {
            return Err(LexError::UnterminatedString { start: lit_start });
        }
        let lit_end = self.span_end();
        let text = String::from_utf8(bytes).map_err(|_| LexError::InvalidUtf8 {
            start: lit_start,
            end: lit_end,
        })?;
        Ok(TokenKind::String(text))
    }

    fn next_char(&mut self) -> Result<Option<char>, LexError> {
        if self.is_at_end() {
            return Ok(None);
        }
        let rest = &self.source[self.cursor..];
        let Some(ch) = rest.chars().next() else {
            return Err(LexError::InvalidUtf8 {
                start: self.span_start(),
                end: self.span_end(),
            });
        };
        self.cursor += ch.len_utf8();
        Ok(Some(ch))
    }

    fn read_byte_char_content(&mut self) -> Result<u8, LexError> {
        if self.is_at_end() {
            return Err(LexError::UnterminatedString {
                start: self.span_start(),
            });
        }
        if self.peek_byte() == Some(b'\\') {
            self.read_escape()
        } else {
            let b = self
                .peek_byte()
                .ok_or_else(|| LexError::UnterminatedString {
                    start: self.span_start(),
                })?;
            if b > 0x7F {
                return Err(self.unexpected_char());
            }
            self.advance();
            Ok(b)
        }
    }

    fn read_byte_string_char(&mut self) -> Result<u8, LexError> {
        if self.peek_byte() == Some(b'\\') {
            self.read_escape()
        } else {
            let b = self
                .peek_byte()
                .ok_or_else(|| LexError::UnterminatedString {
                    start: self.span_start(),
                })?;
            if b == b'"' || b == b'\\' || b > 0x7F {
                return Err(self.unexpected_char());
            }
            self.advance();
            Ok(b)
        }
    }

    fn read_escape(&mut self) -> Result<u8, LexError> {
        let escape_start = self.cursor;
        self.advance();
        match self.peek_byte() {
            Some(b'n') => {
                self.advance();
                Ok(b'\n')
            }
            Some(b'r') => {
                self.advance();
                Ok(b'\r')
            }
            Some(b't') => {
                self.advance();
                Ok(b'\t')
            }
            Some(b'0') => {
                self.advance();
                Ok(0)
            }
            Some(b'\\') => {
                self.advance();
                Ok(b'\\')
            }
            Some(b'\'') => {
                self.advance();
                Ok(b'\'')
            }
            Some(b'"') => {
                self.advance();
                Ok(b'"')
            }
            Some(b'x') => {
                self.advance();
                let hi = self.peek_byte().filter(|b| is_hex_digit(*b)).ok_or(
                    LexError::InvalidEscape {
                        offset: u32::try_from(escape_start).unwrap_or(u32::MAX),
                    },
                )?;
                self.advance();
                let lo = self.peek_byte().filter(|b| is_hex_digit(*b)).ok_or(
                    LexError::InvalidEscape {
                        offset: u32::try_from(escape_start).unwrap_or(u32::MAX),
                    },
                )?;
                self.advance();
                let value = (hex_value(hi) << 4) | hex_value(lo);
                Ok(value)
            }
            _ => Err(LexError::InvalidEscape {
                offset: u32::try_from(escape_start).unwrap_or(u32::MAX),
            }),
        }
    }

    fn skip_trivia(&mut self) -> Result<(), LexError> {
        loop {
            while self.peek_byte().is_some_and(is_whitespace) {
                self.advance();
            }
            if self.peek_byte() == Some(b'/') && self.peek_byte_at(1) == Some(b'/') {
                if self.peek_byte_at(2) == Some(b'/') {
                    self.skip_block_comment()?;
                } else {
                    self.skip_line_comment();
                }
            } else {
                break;
            }
        }
        Ok(())
    }

    fn skip_line_comment(&mut self) {
        self.advance();
        self.advance();
        while let Some(b) = self.peek_byte() {
            if b == b'\n' {
                self.advance();
                break;
            }
            if b == b'\r' {
                self.advance();
                if self.peek_byte() == Some(b'\n') {
                    self.advance();
                }
                break;
            }
            self.advance();
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        let start = self.cursor;
        self.advance();
        self.advance();
        self.advance();
        loop {
            if self.is_at_end() {
                return Err(LexError::UnterminatedBlockComment {
                    start: u32::try_from(start).unwrap_or(u32::MAX),
                });
            }
            if self.peek_byte() == Some(b'/')
                && self.peek_byte_at(1) == Some(b'/')
                && self.peek_byte_at(2) == Some(b'/')
            {
                self.advance();
                self.advance();
                self.advance();
                return Ok(());
            }
            self.advance();
        }
    }

    fn make_token(&self, kind: TokenKind<'src>) -> Token<'src> {
        Token::new(kind, Span::new(self.span_start(), self.span_end()))
    }

    fn current_lexeme(&self) -> &'src str {
        &self.source[self.start..self.cursor]
    }

    fn check_lexeme_len(&self) -> Result<(), LexError> {
        let len = self.cursor - self.start;
        if len > MAX_LEXEME_LEN {
            return Err(LexError::LexemeTooLong {
                start: self.span_start(),
                end: self.span_end(),
            });
        }
        Ok(())
    }

    fn span_start(&self) -> u32 {
        u32::try_from(self.start).unwrap_or(u32::MAX)
    }

    fn span_end(&self) -> u32 {
        u32::try_from(self.cursor).unwrap_or(u32::MAX)
    }

    fn invalid_int(&self) -> LexError {
        LexError::InvalidInt {
            start: self.span_start(),
            end: self.span_end(),
        }
    }

    fn invalid_float(&self) -> LexError {
        LexError::InvalidFloat {
            start: self.span_start(),
            end: self.span_end(),
        }
    }

    fn unexpected_char(&self) -> LexError {
        let ch = self.current_char().unwrap_or('\0');
        LexError::UnexpectedChar {
            ch,
            offset: self.span_start(),
        }
    }

    fn unexpected_eof(&self) -> LexError {
        LexError::UnexpectedChar {
            ch: '\0',
            offset: self.span_start(),
        }
    }

    fn is_at_end(&self) -> bool {
        self.cursor >= self.source.len()
    }

    fn peek_byte(&self) -> Option<u8> {
        self.source.as_bytes().get(self.cursor).copied()
    }

    fn peek_byte_at(&self, offset: usize) -> Option<u8> {
        self.source.as_bytes().get(self.cursor + offset).copied()
    }

    fn advance(&mut self) {
        if !self.is_at_end() {
            self.cursor += 1;
        }
    }

    fn consume_byte(&mut self, expected: u8) -> bool {
        if self.peek_byte() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn consume_bytes(&mut self, expected: &[u8]) -> bool {
        let start = self.cursor;
        for &b in expected {
            if self.peek_byte() != Some(b) {
                self.cursor = start;
                return false;
            }
            self.advance();
        }
        true
    }

    fn current_char(&self) -> Option<char> {
        self.source[self.cursor..].chars().next()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntegerBase {
    Decimal,
    Hex,
    Binary,
}

/// Tokenizes all of `source`, including a final [`TokenKind::Eof`].
///
/// # Errors
///
/// Returns the first [`LexError`] encountered.
pub fn lex(source: &str) -> Result<Vec<Token<'_>>, LexError> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::with_capacity(source.len() / 4 + 1);
    loop {
        let token = lexer.next_token()?;
        let is_eof = token.kind == TokenKind::Eof;
        tokens.push(token);
        if is_eof {
            break;
        }
    }
    Ok(tokens)
}

fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

fn is_snake_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_snake_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_ascii_upper(b: u8) -> bool {
    b.is_ascii_uppercase()
}

fn is_hex_digit(b: u8) -> bool {
    b.is_ascii_digit() || matches!(b, b'a'..=b'f' | b'A'..=b'F')
}

fn hex_value(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

fn hex_digit_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn binary_digit_value(b: u8) -> Option<u8> {
    match b {
        b'0' => Some(0),
        b'1' => Some(1),
        _ => None,
    }
}

/// Parses a base-10 integer lexeme, skipping `_` separators without allocating.
fn parse_decimal_integer(s: &str) -> Result<i128, ()> {
    parse_radix_integer(s, 10, decimal_digit_value)
}

fn decimal_digit_value(b: u8) -> Option<u8> {
    b.is_ascii_digit().then_some(b - b'0')
}

/// Parses an integer lexeme in the given radix, skipping `_` separators without allocating.
fn parse_radix_integer(s: &str, radix: u32, digit_value: fn(u8) -> Option<u8>) -> Result<i128, ()> {
    let radix = i128::from(radix);
    let mut value: i128 = 0;
    let mut seen_digit = false;
    for &b in s.as_bytes() {
        if b == b'_' {
            continue;
        }
        let digit = digit_value(b).ok_or(())?;
        seen_digit = true;
        value = value
            .checked_mul(radix)
            .and_then(|v| v.checked_add(i128::from(digit)))
            .ok_or(())?;
    }
    if seen_digit { Ok(value) } else { Err(()) }
}

/// Parses a float lexeme, skipping `_` separators without allocating.
fn parse_float_literal(s: &str) -> Result<f64, ()> {
    let exp_pos = s.bytes().position(|b| b == b'e' || b == b'E');
    let (mantissa, exponent) = match exp_pos {
        Some(pos) => (&s[..pos], &s[pos + 1..]),
        None => (s, ""),
    };
    let mantissa = parse_float_mantissa(mantissa)?;
    if exponent.is_empty() {
        return Ok(mantissa);
    }
    let exp = parse_float_exponent(exponent)?;
    Ok(mantissa * 10f64.powi(exp))
}

fn parse_float_mantissa(s: &str) -> Result<f64, ()> {
    let mut value = 0.0;
    let mut frac_divisor = 1.0;
    let mut in_frac = false;
    let mut seen_digit = false;
    for &b in s.as_bytes() {
        match b {
            b'_' => {}
            b'0'..=b'9' => {
                seen_digit = true;
                let digit = f64::from(b - b'0');
                if in_frac {
                    frac_divisor *= 10.0;
                    value += digit / frac_divisor;
                } else {
                    value = value.mul_add(10.0, digit);
                }
            }
            b'.' => {
                if in_frac {
                    return Err(());
                }
                in_frac = true;
            }
            _ => return Err(()),
        }
    }
    if seen_digit { Ok(value) } else { Err(()) }
}

fn parse_float_exponent(s: &str) -> Result<i32, ()> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b'_' {
        i += 1;
    }
    let negative = if i < bytes.len() && bytes[i] == b'-' {
        i += 1;
        true
    } else if i < bytes.len() && bytes[i] == b'+' {
        i += 1;
        false
    } else {
        false
    };
    let mut exp: i32 = 0;
    let mut seen_digit = false;
    while i < bytes.len() {
        match bytes[i] {
            b'_' => {}
            b'0'..=b'9' => {
                seen_digit = true;
                exp = exp
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(i32::from(bytes[i] - b'0')))
                    .ok_or(())?;
            }
            _ => return Err(()),
        }
        i += 1;
    }
    if seen_digit {
        Ok(if negative { -exp } else { exp })
    } else {
        Err(())
    }
}

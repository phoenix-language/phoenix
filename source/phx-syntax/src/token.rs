//! Phoenix token definitions.
//!
//! [`Token`] and [`TokenKind`] describe the lexical vocabulary produced by the
//! lexer. Shapes follow `docs/design/grammar.ebnf`; reserved words are listed in
//! `docs/design/grammer.md`.

use phx_diagnostics::Span;

/// Suffix on an integer literal (`u` → unsigned default type per EBNF).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntegerSuffix {
    /// Default signed literal (`s32` at typeck).
    None,
    /// Trailing `u` suffix (`u32` at typeck).
    Unsigned,
}

/// Optional explicit float type suffix on a literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatSuffix {
    /// Default `f32` at typeck.
    None,
    /// Trailing `f32` suffix.
    F32,
    /// Trailing `f64` suffix.
    F64,
}

/// A reserved Phoenix keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Keyword {
    /// `const`
    Const,
    /// `var`
    Var,
    /// `if`
    If,
    /// `else`
    Else,
    /// `match`
    Match,
    /// `while`
    While,
    /// `for`
    For,
    /// `loop`
    Loop,
    /// `break`
    Break,
    /// `continue`
    Continue,
    /// `return`
    Return,
    /// `struct`
    Struct,
    /// `enum`
    Enum,
    /// `type`
    Type,
    /// `pub`
    Pub,
    /// `trait`
    Trait,
    /// `impl`
    Impl,
    /// `mod` (child module declaration)
    Mod,
    /// `reexport`
    Reexport,
    /// `as`
    As,
    /// `in`
    In,
    /// `mut`
    Mut,
    /// `self`
    SelfLower,
    /// `Self`
    SelfUpper,
    /// `bool`
    Bool,
    /// `s8`
    S8,
    /// `s16`
    S16,
    /// `s32`
    S32,
    /// `s64`
    S64,
    /// `s128`
    S128,
    /// `u8`
    U8,
    /// `u16`
    U16,
    /// `u32`
    U32,
    /// `u64`
    U64,
    /// `u128`
    U128,
    /// `f32`
    F32,
    /// `f64`
    F64,
    /// `str` UTF-8 text view
    Str,
    /// `unsafe` — unsafe fn or block
    Unsafe,
    /// `extern` — FFI declarations
    Extern,
}

impl Keyword {
    /// Maps a source lexeme to a [`Keyword`] when the text is reserved.
    ///
    /// Function names such as `main` are *not* keywords and do not match here.
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn lookup(s: &str) -> Option<Self> {
        Some(match s {
            "const" => Self::Const,
            "var" => Self::Var,
            "if" => Self::If,
            "else" => Self::Else,
            "match" => Self::Match,
            "while" => Self::While,
            "for" => Self::For,
            "loop" => Self::Loop,
            "break" => Self::Break,
            "continue" => Self::Continue,
            "return" => Self::Return,
            "struct" => Self::Struct,
            "enum" => Self::Enum,
            "type" => Self::Type,
            "pub" => Self::Pub,
            "trait" => Self::Trait,
            "impl" => Self::Impl,
            "mod" => Self::Mod,
            "reexport" => Self::Reexport,
            "as" => Self::As,
            "in" => Self::In,
            "mut" => Self::Mut,
            "self" => Self::SelfLower,
            "Self" => Self::SelfUpper,
            "bool" => Self::Bool,
            "s8" => Self::S8,
            "s16" => Self::S16,
            "s32" => Self::S32,
            "s64" => Self::S64,
            "s128" => Self::S128,
            "u8" => Self::U8,
            "u16" => Self::U16,
            "u32" => Self::U32,
            "u64" => Self::U64,
            "u128" => Self::U128,
            "f32" => Self::F32,
            "f64" => Self::F64,
            "str" => Self::Str,
            "unsafe" => Self::Unsafe,
            "extern" => Self::Extern,
            _ => return None,
        })
    }
}

/// Classification of a single lexical token.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TokenKind<'src> {
    /// End of input sentinel (always last in a full [`crate::lex`] result).
    Eof,
    /// Reserved [`Keyword`].
    Keyword(Keyword),
    /// `snake_case` identifier; lexeme borrows the source buffer.
    Ident(&'src str),
    /// `PascalCase` type identifier; lexeme borrows the source buffer.
    TypeIdent(&'src str),
    /// Boolean literal (`true` or `false`).
    Bool(bool),
    /// Integer literal with optional `u` suffix.
    Integer {
        /// Parsed numeric value.
        value: i128,
        /// `u` suffix, if present.
        suffix: IntegerSuffix,
    },
    /// Floating-point literal with optional `f32` / `f64` suffix.
    Float {
        /// Parsed numeric value.
        value: f64,
        /// Type suffix, if present.
        suffix: FloatSuffix,
    },
    /// Byte character literal (`b'…'`).
    ByteChar(u8),
    /// Byte string literal (`b"…"`).
    ByteString(Vec<u8>),
    /// UTF-8 string literal (`"…"`).
    String(String),
    /// `,`
    Comma,
    /// `;`
    Semicolon,
    /// `.` (field access; not range)
    Dot,
    /// `:`
    Colon,
    /// `::`
    ColonColon,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `=>`
    FatArrow,
    /// `..`
    DotDot,
    /// `..=`
    DotDotEq,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*` (unary deref or multiply; parser disambiguates)
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `**`
    StarStar,
    /// `&&`
    AndAnd,
    /// `||`
    OrOr,
    /// `!`
    Bang,
    /// `&` (bitwise and or borrow; parser disambiguates)
    Amp,
    /// `|`
    Pipe,
    /// `^`
    Caret,
    /// `~`
    Tilde,
    /// `<<`
    Shl,
    /// `>>`
    Shr,
    /// `==`
    EqEq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `=`
    Eq,
    /// `+=`
    PlusEq,
    /// `-=`
    MinusEq,
    /// `*=`
    StarEq,
    /// `/=`
    SlashEq,
    /// `%=`
    PercentEq,
    /// `?` (error propagation)
    Question,
    /// `&mut`
    AmpMut,
    /// `*mut`
    StarMut,
    /// `#[` — start of bracket item attribute
    HashBracket,
    /// `#import`
    HashImport,
    /// `#inline`
    HashInline,
    /// `#cold`
    HashCold,
    /// `#hot`
    HashHot,
    /// `#derive`
    HashDerive,
    /// `@spawn`
    AtSpawn,
    /// `@send`
    AtSend,
    /// `@receive`
    AtReceive,
    /// `@reply`
    AtReply,
}

/// A classified lexeme with its byte span in the source file.
#[derive(Debug, Clone, PartialEq)]
pub struct Token<'src> {
    /// Token classification and payload.
    pub kind: TokenKind<'src>,
    /// Half-open byte range `[start, end)` into the original source text.
    pub span: Span,
}

impl<'src> Token<'src> {
    /// Creates a token with `kind` covering `span`.
    #[must_use]
    pub const fn new(kind: TokenKind<'src>, span: Span) -> Self {
        Self { kind, span }
    }

    /// Returns the token's source text when `kind` carries a borrowed lexeme.
    #[must_use]
    pub fn lexeme<'a>(&'a self, source: &'a str) -> Option<&'a str> {
        match &self.kind {
            TokenKind::Ident(s) | TokenKind::TypeIdent(s) => Some(s),
            _ => source.get(self.span.start as usize..self.span.end as usize),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_lookup_reserved() {
        assert_eq!(Keyword::lookup("const"), Some(Keyword::Const));
        assert_eq!(Keyword::lookup("Self"), Some(Keyword::SelfUpper));
        assert_eq!(Keyword::lookup("s32"), Some(Keyword::S32));
    }

    #[test]
    fn keyword_lookup_non_reserved() {
        assert_eq!(Keyword::lookup("main"), None);
        assert_eq!(Keyword::lookup("my_fn"), None);
        assert_eq!(Keyword::lookup("Point"), None);
    }

    #[test]
    fn token_new_stores_fields() {
        let span = Span::new(0, 4);
        let token = Token::new(TokenKind::Ident("foo"), span);
        assert_eq!(token.span, span);
        assert!(matches!(token.kind, TokenKind::Ident("foo")));
    }
}

//! Phoenix token definitions.
//!
//! [`Token`] and [`TokenKind`] describe the lexical vocabulary produced by the
//! lexer. Shapes follow `docs/design/grammar.ebnf`; reserved words are listed in
//! `docs/design/grammer.md`.
//!
//! ## Pipeline
//!
//! ```text
//! source bytes → lexer → Token stream → parser
//! ```

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
    /// Every reserved keyword variant, for exhaustiveness tests and tooling.
    pub const ALL: [Self; 40] = [
        Self::Const,
        Self::Var,
        Self::If,
        Self::Else,
        Self::Match,
        Self::While,
        Self::For,
        Self::Loop,
        Self::Break,
        Self::Continue,
        Self::Return,
        Self::Struct,
        Self::Enum,
        Self::Type,
        Self::Pub,
        Self::Trait,
        Self::Impl,
        Self::Mod,
        Self::Reexport,
        Self::As,
        Self::In,
        Self::Mut,
        Self::SelfLower,
        Self::SelfUpper,
        Self::Bool,
        Self::S8,
        Self::S16,
        Self::S32,
        Self::S64,
        Self::S128,
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
        Self::U128,
        Self::F32,
        Self::F64,
        Self::Str,
        Self::Unsafe,
        Self::Extern,
    ];

    /// Source spelling for this keyword.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Const => "const",
            Self::Var => "var",
            Self::If => "if",
            Self::Else => "else",
            Self::Match => "match",
            Self::While => "while",
            Self::For => "for",
            Self::Loop => "loop",
            Self::Break => "break",
            Self::Continue => "continue",
            Self::Return => "return",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Type => "type",
            Self::Pub => "pub",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::Mod => "mod",
            Self::Reexport => "reexport",
            Self::As => "as",
            Self::In => "in",
            Self::Mut => "mut",
            Self::SelfLower => "self",
            Self::SelfUpper => "Self",
            Self::Bool => "bool",
            Self::S8 => "s8",
            Self::S16 => "s16",
            Self::S32 => "s32",
            Self::S64 => "s64",
            Self::S128 => "s128",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Str => "str",
            Self::Unsafe => "unsafe",
            Self::Extern => "extern",
        }
    }

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
    /// `#derive` (lexed for diagnostics; use `#[derive(...)]` instead)
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
    fn keyword_all_matches_lookup() {
        for kw in Keyword::ALL {
            assert_eq!(Keyword::lookup(kw.as_str()), Some(kw));
        }
    }

    #[test]
    fn token_new_stores_fields() {
        let span = Span::new(0, 4);
        let token = Token::new(TokenKind::Ident("foo"), span);
        assert_eq!(token.span, span);
        assert!(matches!(token.kind, TokenKind::Ident("foo")));
    }
}

use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
pub enum Token {
    EOF,

    // Keywords
    #[token("declare")]
    Declare,

    #[token("phunc")]
    Phunc,

    #[token("return")]
    Return,

    #[token("if")]
    If,

    #[token("else")]
    Else,

    #[token("while")]
    While,

    #[token("for")]
    For,

    #[token("break")]
    Break,

    #[token("continue")]
    Continue,

    #[token("true")]
    True,

    #[token("false")]
    False,

    #[token("null")]
    Null,

    #[token("match")]
    Match,

    #[token("=>")]
    FatArrow,

    #[token("in")]
    In,

    #[token(".")]
    Dot,

    #[token("..")]
    Range,

    #[token("terminal")]
    Terminal,

    // Operators
    #[token("!=")]
    NotEqual,

    #[token("==")]
    Equal,

    #[token(">")]
    GreaterThan,

    #[token("<")]
    LessThan,

    #[token(">=")]
    GreaterThanOrEqual,

    #[token("<=")]
    LessThanOrEqual,

    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token("*")]
    Multiply,

    #[token("/")]
    Divide,

    #[token("%")]
    Modulo,

    #[token("!")]
    Not,

    #[token("&&")]
    And,

    #[token("||")]
    Or,

    // Other Tokens
    #[regex("[a-zA-Z][a-zA-Z0-9]*")]
    Identifier,

    #[regex(r#""([^"\\]|\\.)*""#)]
    StringLiteral,

    #[regex(r"[0-9]+(\.[0-9]+)?")]
    NumberLiteral,

    #[token("{")]
    LeftBrace,

    #[token("}")]
    RightBrace,

    #[token("(")]
    LeftParenthesis,

    #[token(")")]
    RightParenthesis,

    #[token("[")]
    LeftBracket,

    #[token("]")]
    RightBracket,

    #[token("=")]
    Equals,

    #[token(":")]
    Colon,

    #[token(";")]
    Semicolon,

    #[token(",")]
    Comma,

    // Types
    #[token("number")]
    NumberType,

    #[token("string")]
    StringType,

    #[token("bool")]
    BooleanType,

    #[token("any")]
    AnyType,

    #[token("void")]
    VoidType,

    #[token("vec")]
    VecType,

    #[token("hash")]
    HashType,
}

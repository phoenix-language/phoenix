use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
pub enum Token {
    EOF,
    Illegal,

    // todo - add comments
    Comment(String),

    // Keywords
    #[token("declare")]
    Declare,

    #[token("=")]
    Assign,

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

    #[token("..")]
    Range,

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
    #[regex("[a-zA-Z][a-zA-Z0-9]*", |lexer| lexer.slice().to_owned())]
    Identifier(String),

    #[regex(r#""[^"]*""#, |lexer| lexer.slice()[1..(lexer.slice().len()-1)].to_owned())]
    StringLiteral(String),

    #[regex(r"[0-9]+(\.[0-9]+)?", |lexer| lexer.slice().to_owned())]
    /// The number literal is type f64 so its compatible with multiple number types for the language.
    // The number literal is stored as a string so that it can be parsed into a number later.
    NumberLiteral(String),

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

    #[token(":")]
    Colon,

    #[token(";")]
    Semicolon,

    #[token(",")]
    Comma,

    // Types
    #[token("Number")]
    NumberType,

    #[token("String")]
    StringType,

    #[token("Bool")]
    BooleanType,

    #[token("Any")]
    AnyType,

    #[token("Void")]
    VoidType,

    #[token("Vec")]
    VecType,

    #[token("Hash")]
    HashType,

    #[token("Phunc")]
    PhuncType,

    #[token("Null")]
    NullType,
}

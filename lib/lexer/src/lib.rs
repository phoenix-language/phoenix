// use anyhow::Result;
use logos::Logos;
use tokens::Token;

pub mod tokens;

/// Lexes the input string and returns a vector of the tokens.
pub fn lex(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut lexer = Token::lexer(input);

    while let Some(token) = lexer.next() {
        if let Ok(token) = token {
            tokens.push(token);
        } else {
            eprintln!("Error: Unexpected token at position {}", lexer.span().start);
        }
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_declaration() {
        let input = "declare x: number = 5;";
        let tokens = lex(input);

        let expected_tokens = vec![
            Token::Declare,
            Token::Identifier("x".to_owned()),
            Token::Colon,
            Token::NumberType,
            Token::Assign,
            Token::NumberLiteral("5".to_owned()),
            Token::Semicolon,
        ];

        assert_eq!(tokens, expected_tokens);
    }

    #[test]
    fn test_lexer_arithmetic_expression() {
        let input = "x + 2 * (y - 3)";
        let tokens = lex(input);

        let expected_tokens = vec![
            Token::Identifier("x".to_owned()),
            Token::Plus,
            Token::NumberLiteral("2".to_owned()),
            Token::Multiply,
            Token::LeftParenthesis,
            Token::Identifier("y".to_owned()),
            Token::Minus,
            Token::NumberLiteral("3".to_owned()),
            Token::RightParenthesis,
        ];

        assert_eq!(tokens, expected_tokens);
    }

    #[test]
    fn test_lexer_string_literal() {
        let input = "\"Hello, world!\"";
        let tokens = lex(input);

        let expected_tokens = vec![Token::StringLiteral("Hello, world!".to_owned())];

        assert_eq!(tokens, expected_tokens);
    }

    #[test]
    fn test_lexer_keywords() {
        let input = "if else while for break continue true false null match => in ..";
        let tokens = lex(input);

        let expected_tokens = vec![
            Token::If,
            Token::Else,
            Token::While,
            Token::For,
            Token::Break,
            Token::Continue,
            Token::True,
            Token::False,
            Token::Null,
            Token::Match,
            Token::FatArrow,
            Token::In,
            Token::Range,
        ];

        assert_eq!(tokens, expected_tokens);
    }

    #[test]
    fn test_lexer_operators() {
        let input = "!= == > < >= <= + - * / % ! && ||";
        let tokens = lex(input);

        let expected_tokens = vec![
            Token::NotEqual,
            Token::Equal,
            Token::GreaterThan,
            Token::LessThan,
            Token::GreaterThanOrEqual,
            Token::LessThanOrEqual,
            Token::Plus,
            Token::Minus,
            Token::Multiply,
            Token::Divide,
            Token::Modulo,
            Token::Not,
            Token::And,
            Token::Or,
        ];

        assert_eq!(tokens, expected_tokens);
    }

    #[test]
    fn test_lexer_types() {
        let input = "Number String Bool Any Void Vec Hash Phunc Null";
        let tokens = lex(input);

        let expected_tokens = vec![
            Token::NumberType,
            Token::StringType,
            Token::BooleanType,
            Token::AnyType,
            Token::VoidType,
            Token::VecType,
            Token::HashType,
            Token::PhuncType,
            Token::NullType,
        ];

        assert_eq!(tokens, expected_tokens);
    }
}

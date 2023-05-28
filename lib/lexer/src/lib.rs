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
    fn test_lexer() {
        let input = r#"
            declare x: number = 10;
            declare y: number = 20;
        "#;

        let expected_tokens = vec![
            Token::Declare,
            Token::Identifier(String::from("x")),
            Token::Colon,
            Token::NumberType,
            Token::Equals,
            Token::NumberLiteral(String::from("10")),
            Token::Semicolon,
            Token::Declare,
            Token::Identifier(String::from("y")),
            Token::Colon,
            Token::NumberType,
            Token::Equals,
            Token::NumberLiteral(String::from("20")),
            Token::Semicolon,
        ];

        let tokens = lex(input);

        assert_eq!(tokens, expected_tokens);
    }

    #[test]
    fn test_lexer_string_literals() {
        let input = r#"
        declare message: string = "Hello, world!";
        declare emptyString: string = "";
        declare multilineString: string = "This is
        a multiline
        string.";
    "#;

        let expected_tokens = vec![
            Token::Declare,
            Token::Identifier(String::from("message")),
            Token::Colon,
            Token::StringType,
            Token::Equals,
            Token::StringLiteral(String::from("Hello, world!")),
            Token::Semicolon,
            Token::Declare,
            Token::Identifier(String::from("emptyString")),
            Token::Colon,
            Token::StringType,
            Token::Equals,
            Token::StringLiteral(String::from("")),
            Token::Semicolon,
            Token::Declare,
            Token::Identifier(String::from("multilineString")),
            Token::Colon,
            Token::StringType,
            Token::Equals,
            Token::StringLiteral(String::from(
                "This is\n        a multiline\n        string.",
            )),
            Token::Semicolon,
        ];

        let tokens = lex(input);

        assert_eq!(tokens, expected_tokens);
    }

    #[test]
    fn test_lexer_identifiers() {
        let input = r#"
        declare name: string = "John";
        phunc greet(person: string) {}
        greet(name);
    "#;

        let expected_tokens = vec![
            Token::Declare,
            Token::Identifier(String::from("name")),
            Token::Colon,
            Token::StringType,
            Token::Equals,
            Token::StringLiteral(String::from("John")),
            Token::Semicolon,
            Token::Phunc,
            Token::Identifier(String::from("greet")),
            Token::LeftParenthesis,
            Token::Identifier(String::from("person")),
            Token::Colon,
            Token::StringType,
            Token::RightParenthesis,
            Token::LeftBrace,
            Token::RightBrace,
            Token::Identifier(String::from("greet")),
            Token::LeftParenthesis,
            Token::Identifier(String::from("name")),
            Token::RightParenthesis,
            Token::Semicolon,
        ];

        let tokens = lex(input);

        assert_eq!(tokens, expected_tokens);
    }
}

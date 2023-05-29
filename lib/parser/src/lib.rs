#![allow(unused_must_use)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
// #![allow(unreachable_patterns)]

mod ast;

use std::fmt::format;

use anyhow::Result;
use ast::*;

use lexer::tokens::Token;

type ProgramType = Vec<Statement>;

pub struct Parser {
    tokens: Vec<Token>,
    current_token: Option<Token>,
    current_index: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        let mut parser = Parser {
            tokens,
            current_token: None,
            current_index: 0,
        };
        parser.advance_token();
        parser
    }

    fn advance_token(&mut self) {
        if self.current_index < self.tokens.len() {
            self.current_token = Some(self.tokens[self.current_index].clone());
            self.current_index += 1;
        } else {
            self.current_token = None;
        }
    }

    fn consume_token(&mut self, expected_token: Token) -> Result<(), String> {
        if let Some(token) = &self.current_token {
            if *token == expected_token {
                self.advance_token();
                return Ok(());
            }
        }

        Err(format!(
            "Expected {:?} but found {:?} during token consumption",
            expected_token, self.current_token
        ))
    }

    pub fn parse(&mut self) -> Result<Vec<Statement>, String> {
        let mut program: ProgramType = Vec::new();

        loop {
            let token = &self.tokens[self.current_index];

            match token {
                Token::EOF => break,
                Token::Illegal => todo!("Handle illegal tokens using a custom error handler."),

                Token::Comment(_) => todo!(),

                Token::Declare => self.parse_variable_declaration(&mut program),
                Token::Semicolon => match self.consume_token(Token::Semicolon) {
                    Ok(_) => continue,
                    Err(err) => {
                        return Err(err.to_string());
                    }
                },
                _ => Ok(program.push(Statement::ExpressionStatement(Box::new(
                    self.parse_expression()?,
                )))),
            };
        }

        Ok(program)
    }

    fn parse_variable_declaration(&mut self, program: &mut ProgramType) -> Result<(), String> {
        self.consume_token(Token::Declare)?;

        match &self.current_token {
            Some(Token::Identifier(ident)) => {
                let identifier = ident.clone();
                self.consume_token(Token::Identifier(identifier.clone()))?;

                let datatype = if let Some(Token::Colon) = &self.current_token {
                    self.consume_token(Token::Colon)?;
                    self.parse_type()?
                } else {
                    // Default to AnyType if no explicit type is provided
                    Type::AnyType
                };

                self.consume_token(datatype.clone().into())?;

                let value = if let Some(Token::Assign) = &self.current_token {
                    self.consume_token(Token::Assign)?;
                    self.parse_expression().ok()
                } else {
                    None
                };

                let declaration = Statement::VariableDeclaration {
                    identifier,
                    value: value.unwrap_or_else(|| Expression::Literal(Literal::Null)),
                    datatype,
                };

                program.push(declaration);
            }
            _ => {
                return Err(format!(
                    "Expected identifier on declaration, found: {:?}",
                    self.current_token
                ))
            }
        }

        self.consume_token(Token::Semicolon)?;

        Ok(())
    }

    fn parse_phunctions_declaration(&mut self, program: &mut ProgramType) {
        todo!()
    }

    fn parse_expression(&mut self) -> Result<Expression, String> {
        match &self.current_token {
            Some(Token::NumberLiteral(_)) => self.parse_number_literal().map(Expression::Literal),
            Some(Token::StringLiteral(_)) => self.parse_string_literal().map(Expression::Literal),
            Some(Token::True) => {
                self.advance_token();
                Ok(Expression::Literal(Literal::Boolean(true)))
            }
            Some(Token::False) => {
                self.advance_token();
                Ok(Expression::Literal(Literal::Boolean(false)))
            }
            Some(Token::Null) => {
                self.advance_token();
                Ok(Expression::Literal(Literal::Null))
            }
            Some(Token::Identifier(ident)) => {
                let identifier = ident.clone();
                self.consume_token(Token::Identifier(identifier.clone()))
                    .expect("Expected identifier");

                if let Some(Token::LeftParenthesis) = &self.current_token {
                    // Function call
                    self.consume_token(Token::LeftParenthesis)
                        .expect("Expected '('");
                    let arguments = self.parse_argument_list()?;
                    self.consume_token(Token::RightParenthesis)
                        .expect("Expected ')'");

                    let function_call = Expression::FunctionCall {
                        name: identifier,
                        arguments,
                    };
                    Ok(function_call)
                } else {
                    // Variable identifier
                    let variable_identifier = Expression::Identifier(identifier);
                    Ok(variable_identifier)
                }
            }
            _ => Err(format!(
                "Unexpected token while parsing expression {:?}",
                self.current_token
            )),
        }
    }

    /// Parses a type from the current token
    fn parse_type(&mut self) -> Result<Type, String> {
        match &self.current_token {
            Some(Token::Null) => Ok(Type::NullType),
            Some(Token::NumberType) => Ok(Type::NumberType),
            Some(Token::StringType) => Ok(Type::StringType),
            Some(Token::BooleanType) => Ok(Type::BooleanType),
            Some(Token::VoidType) => Ok(Type::VoidType),
            Some(Token::AnyType) => Ok(Type::AnyType),
            Some(Token::VecType) => {
                self.consume_token(Token::VecType)?;
                let element_type = self.parse_type()?;
                Ok(Type::VecType(Box::new(element_type)))
            }
            Some(Token::HashType) => {
                self.consume_token(Token::HashType)?;
                let value_type = self.parse_type()?;
                Ok(Type::HashType(Box::new(value_type)))
            }

            _ => Err(format!(
                "Unexpected token while parsing type {:?}",
                self.current_token
            )),
        }
    }

    fn parse_number_literal(&mut self) -> Result<Literal, String> {
        match &self.current_token {
            Some(Token::NumberLiteral(number)) => {
                let num = number
                    .parse::<f64>()
                    .map_err(|_| format!("Invalid number literal {}", number))?;
                self.advance_token();
                Ok(Literal::Number(num))
            }
            _ => Err(format!(
                "Expected number literal found {:?}",
                self.current_token
            )),
        }
    }

    fn parse_string_literal(&mut self) -> Result<Literal, String> {
        match &self.current_token {
            Some(Token::StringLiteral(string)) => {
                let value = string.clone();
                self.advance_token();

                Ok(Literal::String(value))
            }
            _ => Err(format!(
                "Expected string literal found {:?}",
                self.current_token
            )),
        }
    }

    fn parse_argument_list(&mut self) -> Result<Vec<Expression>, String> {
        let mut arguments: Vec<Expression> = Vec::new();

        while self.current_token != Some(Token::RightParenthesis) {
            let argument = self.parse_expression()?;
            arguments.push(argument);

            if let Some(Token::Comma) = &self.current_token {
                self.consume_token(Token::Comma)?;
            } else {
                break;
            }
        }

        Ok(arguments)
    }

    /// Converts a NumberLiteral token to a f64. Because of how the lexer stores the types, it needs to a string first.
    fn to_f64(&mut self, t: &Token) -> Result<f64, String> {
        let number_str = match t {
            Token::NumberLiteral(n) => n.clone(),
            _ => return Err("Expected number literal found '{t}'".to_string()),
        };

        match number_str.parse::<f64>() {
            Ok(num) => Ok(num),
            Err(_) => return Err("Invalid number literal. Parse to f64 failed".to_string()),
        }
    }
}

impl Into<Token> for ast::Type {
    fn into(self) -> Token {
        match self {
            ast::Type::NumberType => Token::NumberType,
            ast::Type::StringType => Token::StringType,
            ast::Type::BooleanType => Token::BooleanType,
            ast::Type::VecType(_) => Token::VecType,
            ast::Type::HashType(_) => Token::HashType,
            ast::Type::NullType => Token::Null,
            ast::Type::AnyType => Token::AnyType,
            ast::Type::VoidType => Token::VoidType,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lexer::lex;
    use Type::*;

    #[test]
    fn test_parse_number_literal() {
        let tokens = lex("1234");
        let mut parser = Parser::new(tokens);
        let literal = parser.parse_number_literal().unwrap();
        assert_eq!(literal, Literal::Number(1234.0));
    }

    #[test]
    fn test_parse_string_literal() {
        let tokens = lex(r#""Hello World""#);
        let mut parser = Parser::new(tokens);
        let literal = parser.parse_string_literal().unwrap();
        assert_eq!(literal, Literal::String("Hello World".to_string()));
    }

    #[test]
    fn test_parse_variable_declaration() {
        let tokens = lex("declare x: Number = 1234;");
        let mut parser = Parser::new(tokens);
        let mut program = Vec::new();

        parser.parse_variable_declaration(&mut program).unwrap();

        let expected_program = vec![Statement::VariableDeclaration {
            identifier: "x".to_string(),
            value: Expression::Literal(Literal::Number(1234.0)),
            datatype: NumberType,
        }];

        assert_eq!(program, expected_program);
    }

    #[test]
    fn test_parse_variable_declaration_with_any_type() {
        let tokens = lex("declare x: Any = 1234;");
        let mut parser = Parser::new(tokens);
        let mut program = Vec::new();

        parser.parse_variable_declaration(&mut program).unwrap();

        let expected_program = vec![Statement::VariableDeclaration {
            identifier: "x".to_string(),
            value: Expression::Literal(Literal::Number(1234.0)),
            datatype: AnyType,
        }];

        assert_eq!(program, expected_program);
    }

    #[test]
    fn test_parse_variable_declaration_without_value() {
        let tokens = lex("declare x: Number;");
        let mut parser = Parser::new(tokens);
        let mut program = Vec::new();

        parser.parse_variable_declaration(&mut program).unwrap();

        let expected_program = vec![Statement::VariableDeclaration {
            identifier: "x".to_string(),
            value: Expression::Literal(Literal::Null),
            datatype: NumberType,
        }];

        assert_eq!(program, expected_program);
    }
}

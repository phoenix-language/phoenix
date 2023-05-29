#![allow(unused_must_use)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
// #![allow(unreachable_patterns)]

mod ast;

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
            "Expected {:?} but found {:?}",
            expected_token, self.current_token
        ))
    }

    pub fn parse(&mut self, input: &mut Vec<Token>) -> Result<Vec<Statement>, String> {
        let mut program: ProgramType = Vec::new();

        loop {
            let token = &self.tokens[self.current_index];

            match token {
                Token::EOF => break,
                Token::Illegal => todo!(),
                Token::Comment(_) => todo!(),

                Token::Declare => self.parse_variable_declaration(&mut program),
                Token::Phunc => Ok(self.parse_phunctions_declaration(&mut program)),
                Token::Assign => todo!(),
                Token::Return => todo!(),
                Token::If => todo!(),
                Token::Else => todo!(),
                Token::While => todo!(),
                Token::For => todo!(),
                Token::Break => todo!(),
                Token::Continue => todo!(),
                Token::True => todo!(),
                Token::False => todo!(),
                Token::Null => todo!(),
                Token::Match => todo!(),
                Token::FatArrow => todo!(),
                Token::In => todo!(),
                Token::Range => todo!(),
                Token::NotEqual => todo!(),
                Token::Equal => todo!(),
                Token::GreaterThan => todo!(),
                Token::GreaterThanOrEqual => todo!(),
                Token::LessThan => todo!(),
                Token::LessThanOrEqual => todo!(),
                Token::Plus => todo!(),
                Token::Minus => todo!(),
                Token::Multiply => todo!(),
                Token::Divide => todo!(),
                Token::Modulo => todo!(),
                Token::Not => todo!(),
                Token::And => todo!(),
                Token::Or => todo!(),

                Token::Identifier(_) => todo!(),
                Token::StringLiteral(_) => todo!(),
                Token::NumberLiteral(_) => todo!(),

                Token::LeftBrace => todo!(),
                Token::RightBrace => todo!(),
                Token::LeftParenthesis => todo!(),
                Token::RightParenthesis => todo!(),
                Token::LeftBracket => todo!(),
                Token::RightBracket => todo!(),
                Token::Comma => todo!(),
                Token::Semicolon => match self.consume_token(Token::Semicolon) {
                    Ok(_) => continue,
                    Err(err) => {
                        return Err(err.to_string());
                    }
                },
                Token::Colon => todo!(),

                Token::NumberType => todo!(),
                Token::StringType => todo!(),
                Token::BooleanType => todo!(),
                Token::VoidType => todo!(),
                Token::AnyType => todo!(),
                Token::VecType => todo!(),
                Token::PhuncType => todo!(),
                Token::HashType => todo!(),

                _ => Ok(program.push(Statement::ExpressionStatement(Box::new(
                    Expression::Literal(Literal::Null),
                )))),
            };

            assert_eq!(Token::Semicolon, input.remove(0));
        }

        Ok(program)
    }

    fn parse_variable_declaration(&mut self, program: &mut ProgramType) -> Result<(), String> {
        self.consume_token(Token::Declare)?;

        loop {
            match &self.current_token {
                Some(Token::Identifier(ident)) => {
                    let identifier = ident.clone();
                    self.consume_token(Token::Identifier(identifier.clone()))?;

                    let datatype = match &self.current_token {
                        Some(Token::NumberType) => Type::NumberType,
                        Some(Token::StringType) => Type::StringType,
                        Some(Token::BooleanType) => Type::BooleanType,
                        Some(Token::VoidType) => Type::VoidType,
                        Some(Token::AnyType) => Type::AnyType,
                        Some(Token::VecType) => {
                            self.consume_token(Token::VecType)?;
                            let element_type = self.parse_type()?;
                            Type::VecType(Box::new(element_type))
                        }
                        Some(Token::HashType) => {
                            self.consume_token(Token::HashType)?;
                            let value_type = self.parse_type()?;
                            Type::HashType(Box::new(value_type))
                        }
                        _ => {
                            return Err(format!(
                                "Invalid variable type for identifier '{}'",
                                identifier
                            ))
                        }
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

                    // Check if there is a comma to indicate multiple declarations on the same line
                    if let Some(Token::Comma) = &self.current_token {
                        self.consume_token(Token::Comma)?;
                        continue;
                    }
                }
                _ => {
                    // If the next token is not an identifier, break the loop
                    break;
                }
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
            _ => Err("Unexpected token while parsing expression".to_string()),
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

            _ => Err("Invalid type".to_string()),
        }
    }

    fn parse_number_literal(&mut self) -> Result<Literal, String> {
        match &self.current_token {
            Some(Token::NumberLiteral(number)) => {
                let num = number
                    .parse::<f64>()
                    .map_err(|_| "Invalid number literal".to_string())?;
                self.advance_token();
                Ok(Literal::Number(num))
            }
            _ => Err("Expected number literal".to_string()),
        }
    }

    fn parse_string_literal(&mut self) -> Result<Literal, String> {
        match &self.current_token {
            Some(Token::StringLiteral(string)) => {
                let value = string.clone();
                self.advance_token();

                Ok(Literal::String(value))
            }
            _ => Err("Expected string literal".to_string()),
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
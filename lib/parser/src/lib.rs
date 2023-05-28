#![allow(unused_must_use)]
#![allow(dead_code)]
#![allow(unused_variables)]

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

    pub fn parse(&mut self, input: &mut Vec<Token>) -> Result<Vec<Statement>> {
        let mut program: ProgramType = Vec::new();

        loop {
            let token = &self.tokens[self.current_index];

            match token {
                Token::EOF => break,
                Token::Illegal => todo!(),
                Token::Comment(_) => todo!(),

                Token::Declare => self.parse_declarations(input, &mut program),
                Token::Phunc => self.parse_phunctions(input, &mut program),
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
                Token::Dot => todo!(),
                Token::Range => todo!(),
                Token::Terminal => todo!(),
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
                Token::Equals => todo!(),
                Token::Comma => todo!(),
                Token::Semicolon => todo!(),
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

    fn parse_declarations(
        &mut self,
        input: &mut Vec<Token>,
        program: &mut ProgramType,
    ) -> Result<()> {
        todo!();
    }

    fn parse_phunctions(
        &mut self,
        input: &mut Vec<Token>,
        program: &mut ProgramType,
    ) -> Result<()> {
        todo!()
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

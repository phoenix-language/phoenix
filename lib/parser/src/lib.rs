mod ast;

use anyhow::{Result, Error};
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

    fn consume_token(&mut self, expected_token: &Token) -> Result<(), String> {
        if let Some(token) = &self.current_token {
            if token == expected_token {
                self.advance_token();
                return Ok(());
            }
        }
        
        Err(format!(
            "Expected {:?} but found {:?}",
            expected_token, self.current_token
        ))
    }

    pub fn parse(&mut self) -> Result<Vec<Statement>> {
        let mut program: ProgramType = Vec::new();

        loop {
            let token = &self.tokens[self.current_index];

            match token {
                Token::EOF => break,
                Token::Illegal => todo!(),
                Token::Declare => self.parse_declarations(&mut program),
                Token::Phunc => self.parse_phunctions(&mut program),
            };

            self.consume_token(token);
        }

        Ok(program)
    }

    fn parse_declarations(&mut self, program: &mut ProgramType) -> Result<()> {
        self.consume_token(Token::Declare);

        Ok(())
    }

    fn parse_phunctions(&mut self, program: &mut ProgramType) -> Result<()> {
        todo!()
    }

    fn to_f64(&mut self, t: Token) -> Result<f64> {
        let number_str = match &self.current_token {
            Some(Token::NumberLiteral(n)) => n.clone(),
           _ => return Err("Expected number literal found '{t}'".to_string()),
        };

        match number_str.parse::<f64>() {
            Ok(num) => Ok(num),
            Err(_) => return Err("Invalid number literal. Parse to f64 failed".to_string()),
        }
    }
}

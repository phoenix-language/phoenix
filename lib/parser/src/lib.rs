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
        let mut parser = Self {
            tokens,
            current_token: None,
            current_index: 0,
        };
        parser.advance_token();
        parser
    }

    /// Reads the next token from the token stream and stores it in the current_token field.
    ///
    /// If there are no more tokens, the current_token field is set to None.
    fn advance_token(&mut self) {
        if self.current_index < self.tokens.len() {
            self.current_token = Some(self.tokens[self.current_index].clone());
            self.current_index += 1;
        } else {
            self.current_token = None;
        }
    }

    /// Consumes the current token if it matches the expected token.
    ///
    /// This function is called after a token has been parsed.
    fn consume_token(&mut self, expected_token: Token) -> Result<(), String> {
        if let Some(token) = &self.current_token {
            if *token == expected_token {
                self.advance_token();
                return Ok(());
            }
        }

        Err(format!(
            "Expected {:?} but found {:?} during the consume_token function",
            expected_token, self.current_token
        ))
    }

    /// Parses the tokens into an AST.
    pub fn parse(&mut self) -> Result<Vec<Statement>, String> {
        let mut program: ProgramType = Vec::new();

        // Loop through each token and parse it into a statement for the AST.
        loop {
            let token = &self.tokens[self.current_index];

            match token {
                Token::EOF => break,
                Token::Illegal => todo!("Handle illegal tokens using a custom error handler."),

                Token::Comment(_) => todo!(),
                Token::Identifier(_) => {
                    if self.lookahead_token(1) == Some(Token::LeftParenthesis) {
                        self.parse_phunctions_declaration(&mut program)
                    } else {
                        self.parse_variable_declaration(&mut program)
                    }
                }
                Token::Return => self.parse_return_statement(&mut program),
                Token::If => todo!("Parse if statement"),
                Token::While => todo!("Parse while statement"),
                Token::For => todo!("Parse for statement"),
                Token::Break => todo!("Parse break statement"),
                Token::Declare => self.parse_variable_declaration(&mut program),
                Token::Phunc => self.parse_phunctions_declaration(&mut program),
                Token::Return => self.parse_return_statement(&mut program),
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
                match self.consume_token(Token::Identifier(identifier.clone())) {
                    Ok(_) => (),
                    Err(err) => return Err(err),
                }

                let datatype = if let Some(Token::Colon) = &self.current_token {
                    match self.consume_token(Token::Colon) {
                        Ok(_) => (),
                        Err(err) => return Err(err),
                    }
                    match self.parse_type() {
                        Ok(datatype) => datatype,
                        Err(err) => return Err(err),
                    }
                } else {
                    // Default to AnyType if no explicit type is provided
                    Type::AnyType
                };

                match self.consume_token(datatype.clone().into()) {
                    Ok(_) => (),
                    Err(err) => return Err(err),
                }

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

    /// Parses a return statement for functions.
    fn parse_return_statement(&mut self, program: &mut ProgramType) -> Result<(), String> {
        self.consume_token(Token::Return)?;

        let expression = if let Some(Token::Semicolon) = &self.current_token {
            None
        } else {
            Some(self.parse_expression()?)
        };

        self.consume_token(Token::Semicolon)?;

        let return_statement = Statement::ReturnStatement { expression };

        program.push(return_statement);

        Ok(())
    }

    fn parse_phunctions_declaration(&mut self, program: &mut ProgramType) -> Result<(), String> {
        self.consume_token(Token::Phunc)?;

        let phunc_name = if let Some(Token::Identifier(name)) = self.current_token.clone() {
            self.advance_token();
            name
        } else {
            return Err(format!(
                "Expected identifier for function name, found: {:?}",
                self.current_token
            ));
        };

        self.consume_token(Token::LeftParenthesis)?;

        let mut parameters: Vec<FunctionParameter> = vec![];

        // Parse function parameters
        // If the next token is a right parenthesis, then the function takes no parameters and we can break out of the loop.
        while let Some(token) = self.current_token.clone() {
            match token {
                Token::RightParenthesis => {
                    self.advance_token();
                    break;
                }
                Token::Identifier(identifier) => {
                    self.advance_token();

                    self.consume_token(Token::Colon)?;

                    // Expecting type after colon
                    let datatype = self.parse_type()?;

                    let param = FunctionParameter {
                        identifier,
                        datatype,
                    };

                    parameters.push(param);

                    self.advance_token();

                    // Expecting comma or right parenthesis after parameter
                    if let Some(Token::Comma) = self.current_token.clone() {
                        self.advance_token();
                    } else if let Some(Token::RightParenthesis) = self.current_token.clone() {
                        self.advance_token();
                        break;
                    } else {
                        return Err(format!(
                            "Expected comma or right parenthesis after parameter, found: {:?}",
                            self.current_token
                        ));
                    }
                }
                _ => {
                    return Err(format!(
                        "Expected identifier for parameter, found: {:?}",
                        self.current_token
                    ));
                }
            }
        }

        // Expecting function return type
        let return_type = self.parse_type()?;

        // Expecting open brace for function body
        self.consume_token(Token::LeftBrace)?;

        // Parse function body
        let mut body = Vec::new();

        while let Some(token) = self.current_token.clone() {
            match token {
                Token::RightBrace => {
                    self.advance_token();
                    break;
                }
                _ => {
                    let statement = match self.parse() {
                        Ok(statement) => statement,
                        Err(err) => return Err(err),
                    };

                    body.extend(statement);
                }
            }
        }

        // todo - functions with public can be imported and exported into other files
        // for now, all functions are private by default
        let phunc = Statement::FunctionDeclaration {
            visibility: Visibility::Private,
            name: phunc_name,
            parameters,
            return_type,
            body,
        };

        program.push(phunc);

        println!("Program: {:?}", program);

        Ok(())
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

    /// Looks ahead in the token stream by n tokens.
    /// 
    /// This is helpful for looking at tokens and there value before parsing them.
    /// 
    /// Returns None if the index is out of bounds.
    fn lookahead_token(&self, n: usize) -> Option<Token> {
        let index = self.current_index + n;
        if index < self.tokens.len() {
            Some(self.tokens[index].clone())
        } else {
            None
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

    #[test]
    // todo - get functions working!
    fn test_parse_phunctions_declaration() {
        let tokens = lex("phunc addTwoNumbers(x: Number, y: Number): Number { return x + y; };");
        println!("Token Stream: {:?}", tokens); // Log the token stream

        let mut parser = Parser::new(tokens);

        let mut program = Vec::new();

        let result = parser.parse_phunctions_declaration(&mut program);

        let expected_parameters = vec![
            FunctionParameter {
                identifier: "x".to_string(),
                datatype: Type::NumberType,
            },
            FunctionParameter {
                identifier: "y".to_string(),
                datatype: Type::NumberType,
            },
        ];

        let return_statement = Statement::ReturnStatement {
            expression: Some(Expression::BinaryOperation {
                operator: Operator::Add,
                left: Box::new(Expression::Identifier(String::from("x"))),
                right: Box::new(Expression::Identifier(String::from("y"))),
            }),
        };

        let expected_statement = Statement::FunctionDeclaration {
            visibility: Visibility::Private,
            name: String::from("addTwoNumbers"),
            parameters: expected_parameters,
            return_type: Type::NumberType,
            body: vec![return_statement],
        };

        assert_eq!(program, vec![expected_statement]);
    }
}

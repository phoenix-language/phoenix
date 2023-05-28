use lexer::tokens::Token;

mod ast;

use crate::ast::*;

pub fn parse(input: &mut Vec<Token>) -> Vec<Statement> {
    let mut program = vec![];

    loop {
        let token = &input[0];

        match token {
           _ => program.push(
                Statement::ExpressionStatement(
                    Box::new(Expression::Literal(
                        Literal::Boolean(true)
                    ))
                )
            )
        }
    }
}

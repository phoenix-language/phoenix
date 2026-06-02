//! Statement and block parsing.

use phx_diagnostics::ExpectedToken;

use crate::ast::stmt::{Block, BlockItem, Stmt};
use crate::ast::{BlockNode, ExprNode, Node, StmtNode};
use crate::parser::Parser;
use crate::token::{Keyword, TokenKind};

impl Parser<'_> {
    pub(crate) fn parse_block(&mut self) -> Result<BlockNode, ParseError> {
        let start = self.pos;
        self.expect_kind(ExpectedToken::Punct("{"), &TokenKind::LBrace)?;
        let mut items = Vec::new();
        while !self.eat_kind(&TokenKind::RBrace) {
            items.push(self.parse_block_item()?);
        }
        Ok(Node::new(Block { items }, self.span_from(start)))
    }

    fn parse_block_item(&mut self) -> Result<BlockItem, ParseError> {
        if self.is_block_expr_start() {
            let expr = self.parse_expr_without_semi()?;
            let _ = self.eat_kind(&TokenKind::Semicolon);
            return Ok(BlockItem::Expr(expr));
        }
        if self.is_stmt_keyword() {
            let stmt = self.parse_stmt()?;
            return Ok(BlockItem::Stmt(stmt.inner));
        }
        let expr = self.parse_expr()?;
        if self.peek_kind() == TokenKind::RBrace {
            return Ok(BlockItem::Expr(expr));
        }
        self.expect_semi()?;
        Ok(BlockItem::Stmt(Stmt::Expr(expr)))
    }

    fn is_stmt_keyword(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Keyword(
                Keyword::Const
                    | Keyword::Var
                    | Keyword::Return
                    | Keyword::Break
                    | Keyword::Continue
                    | Keyword::While
                    | Keyword::For
                    | Keyword::Loop
                    | Keyword::Given
            ) | TokenKind::HashUnsafe
        )
    }

    fn is_block_expr_start(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Keyword(Keyword::If | Keyword::Match) | TokenKind::LBrace
        )
    }

    fn parse_expr_without_semi(&mut self) -> Result<ExprNode, ParseError> {
        self.parse_expr_or_block_value()
    }

    pub(crate) fn parse_stmt(&mut self) -> Result<StmtNode, ParseError> {
        let start = self.pos;
        let stmt = match self.peek_kind() {
            TokenKind::Keyword(Keyword::Const) => {
                self.bump();
                let name = self.parse_ident()?;
                let ty = if self.eat_kind(&TokenKind::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                self.expect_kind(ExpectedToken::Punct("="), &TokenKind::Eq)?;
                let init = self.parse_expr()?;
                self.expect_semi()?;
                Stmt::Const { name, ty, init }
            }
            TokenKind::Keyword(Keyword::Var) => {
                self.bump();
                let name = self.parse_ident()?;
                self.expect_kind(ExpectedToken::Punct(":"), &TokenKind::Colon)?;
                let ty = self.parse_type()?;
                self.expect_kind(ExpectedToken::Punct("="), &TokenKind::Eq)?;
                let init = self.parse_expr()?;
                self.expect_semi()?;
                Stmt::Var { name, ty, init }
            }
            TokenKind::Keyword(Keyword::Return) => {
                self.bump();
                let value = if self.peek_kind() == TokenKind::Semicolon {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.expect_semi()?;
                Stmt::Return(value)
            }
            TokenKind::Keyword(Keyword::Break) => {
                self.bump();
                let value = if self.peek_kind() == TokenKind::Semicolon {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.expect_semi()?;
                Stmt::Break(value)
            }
            TokenKind::Keyword(Keyword::Continue) => {
                self.bump();
                self.expect_semi()?;
                Stmt::Continue
            }
            TokenKind::Keyword(Keyword::While) => {
                self.bump();
                let cond = self.parse_expr()?;
                let body = self.parse_block()?;
                let _ = self.eat_kind(&TokenKind::Semicolon);
                Stmt::While { cond, body }
            }
            TokenKind::Keyword(Keyword::For) => {
                return Err(self.reject_unsupported("for-in loop"));
            }
            TokenKind::Keyword(Keyword::Loop) => {
                self.bump();
                let body = self.parse_block()?;
                let _ = self.eat_kind(&TokenKind::Semicolon);
                Stmt::Loop(body)
            }
            TokenKind::Keyword(Keyword::Given) => {
                self.bump();
                let pattern = self.parse_pattern()?;
                self.expect_kind(ExpectedToken::Punct("="), &TokenKind::Eq)?;
                let scrutinee = self.parse_expr()?;
                let body = self.parse_block()?;
                let _ = self.eat_kind(&TokenKind::Semicolon);
                Stmt::Given {
                    pattern,
                    scrutinee,
                    body,
                }
            }
            TokenKind::HashUnsafe => {
                self.bump();
                let body = self.parse_block()?;
                let _ = self.eat_kind(&TokenKind::Semicolon);
                Stmt::Unsafe(body)
            }
            _ => {
                let expr = self.parse_expr()?;
                if matches!(expr.inner, crate::ast::Expr::Assign { .. }) {
                    self.expect_semi()?;
                    Stmt::Assign { expr }
                } else {
                    self.expect_semi()?;
                    Stmt::Expr(expr)
                }
            }
        };
        Ok(Node::new(stmt, self.span_from(start)))
    }
}

use phx_diagnostics::ParseError;

//! Statement and block parsing.
//!
//! Blocks mix statements (with `;`) and optional trailing expressions. Control flow includes
//! `if`/`match` as expressions via [`Parser::parse_expr_without_semi`].

use phx_diagnostics::ExpectedToken;

use crate::ast::stmt::{Block, BlockItem, Stmt};
use crate::ast::{BlockNode, ExprNode, StmtNode};
use crate::parser::Parser;
use crate::token::{Keyword, TokenKind};

impl Parser<'_> {
    /// Parses `{ items… }` as a [`Block`].
    pub(crate) fn parse_block(&mut self) -> Result<BlockNode, ParseError> {
        let start = self.pos;
        self.expect_kind(ExpectedToken::Punct("{"), &TokenKind::LBrace)?;
        let mut items = Vec::new();
        while !self.eat_kind(&TokenKind::RBrace) {
            if self.at_end() {
                break;
            }
            match self.parse_block_item() {
                Ok(item) => items.push(item),
                Err(e) => {
                    if self.in_recovery_mode() {
                        self.record_error(e);
                        self.sync_stmt();
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        Ok(self.node(Block { items }, self.span_from(start)))
    }

    /// Parses one block item (stmt, trailing expr, or expr-as-stmt).
    fn parse_block_item(&mut self) -> Result<BlockItem, ParseError> {
        if matches!(self.peek_kind(), TokenKind::HashImport) {
            let imp = self.parse_import()?;
            return Ok(BlockItem::Import(imp));
        }
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

    /// Returns `true` when the next token starts a statement keyword or `#unsafe`.
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

    /// Returns `true` when the next token can start a block-valued expression.
    fn is_block_expr_start(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Keyword(Keyword::If | Keyword::Match) | TokenKind::LBrace
        )
    }

    /// Parses an expression that may end with `}` without a semicolon.
    fn parse_expr_without_semi(&mut self) -> Result<ExprNode, ParseError> {
        self.parse_expr_or_block_value()
    }

    /// Parses a single statement (must include `;` where required by grammar).
    #[allow(clippy::too_many_lines)]
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
                let init = self.parse_expr_without_semi()?;
                self.expect_semi()?;
                Stmt::Const { name, ty, init }
            }
            TokenKind::Keyword(Keyword::Var) => {
                self.bump();
                let name = self.parse_ident()?;
                self.expect_kind(ExpectedToken::Punct(":"), &TokenKind::Colon)?;
                let ty = self.parse_type()?;
                self.expect_kind(ExpectedToken::Punct("="), &TokenKind::Eq)?;
                let init = self.parse_expr_without_semi()?;
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
                let span = self.current_span();
                self.bump();
                let value = if self.peek_kind() == TokenKind::Semicolon {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.expect_semi()?;
                Stmt::Break { value, span }
            }
            TokenKind::Keyword(Keyword::Continue) => {
                let span = self.current_span();
                self.bump();
                self.expect_semi()?;
                Stmt::Continue { span }
            }
            TokenKind::Keyword(Keyword::While) => {
                self.bump();
                let cond = self.parse_logical_or_expr()?;
                let body = self.parse_block()?;
                let _ = self.eat_kind(&TokenKind::Semicolon);
                Stmt::While { cond, body }
            }
            TokenKind::Keyword(Keyword::For) => {
                self.bump();
                let binding = self.parse_ident()?;
                if !self.eat_keyword(Keyword::In) {
                    return Err(self.error_unexpected(ExpectedToken::Token));
                }
                let iter = self.parse_expr()?;
                let body = self.parse_block()?;
                let _ = self.eat_kind(&TokenKind::Semicolon);
                Stmt::ForIn {
                    binding,
                    iter,
                    body,
                }
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
        Ok(self.node(stmt, self.span_from(start)))
    }
}

use phx_diagnostics::ParseError;

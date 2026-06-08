//! Pattern parsing for `match`, `if const` / `if var`, and bindings.
//!
//! Covers wildcards, literals, ident bindings, struct/tuple patterns, and enum variant patterns.

use phx_diagnostics::ExpectedToken;

use crate::ast::pat::{MatchArm, Pattern, StructPatternField};
use crate::ast::{ExprNode, PatternNode};
use crate::parser::Parser;
use crate::token::{Keyword, TokenKind};

impl Parser<'_> {
    /// Parses one [`Pattern`].
    pub(crate) fn parse_pattern(&mut self) -> Result<PatternNode, ParseError> {
        let start = self.pos;
        match self.peek_kind() {
            TokenKind::Ident("_") => {
                self.bump();
                Ok(self.node(Pattern::Wildcard, self.span_from(start)))
            }
            TokenKind::Integer { .. }
            | TokenKind::Float { .. }
            | TokenKind::Bool(_)
            | TokenKind::ByteChar(_)
            | TokenKind::ByteString(_)
            | TokenKind::String(_) => {
                let lit = self.parse_literal()?;
                Ok(self.node(Pattern::Literal(lit), self.span_from(start)))
            }
            TokenKind::TypeIdent(_) => self.parse_type_pattern(start),
            TokenKind::Ident(_) => {
                let id = self.parse_ident()?;
                Ok(self.node(Pattern::Ident(id), self.span_from(start)))
            }
            _ => Err(ParseError::InvalidPattern {
                span: self.current_span(),
            }),
        }
    }

    /// Parses `TypeName { … }` or `TypeName(…)` variant patterns.
    fn parse_type_pattern(&mut self, start: usize) -> Result<PatternNode, ParseError> {
        let name = self.parse_type_name()?;
        if self.eat_kind(&TokenKind::LBrace) {
            let fields = self.parse_struct_pattern_fields()?;
            return Ok(self.node(Pattern::Struct { name, fields }, self.span_from(start)));
        }
        if self.eat_kind(&TokenKind::LParen) {
            let mut patterns = vec![self.parse_pattern()?];
            while self.eat_kind(&TokenKind::Comma) {
                patterns.push(self.parse_pattern()?);
            }
            self.expect_kind(ExpectedToken::Punct(")"), &TokenKind::RParen)?;
            return Ok(self.node(Pattern::Tuple { name, patterns }, self.span_from(start)));
        }
        let pat_span = self.span_from(start);
        let pat_id = self.alloc_node_id();
        Ok(self.node(
            Pattern::Ident(crate::ast::Ident {
                symbol: name.symbol,
                span: pat_span,
                id: pat_id,
            }),
            pat_span,
        ))
    }

    fn parse_struct_pattern_fields(&mut self) -> Result<Vec<StructPatternField>, ParseError> {
        let mut fields = Vec::new();
        if self.eat_kind(&TokenKind::RBrace) {
            return Ok(fields);
        }
        loop {
            let name = self.parse_ident()?;
            let pattern = if self.eat_kind(&TokenKind::Colon) {
                Some(Box::new(self.parse_pattern()?))
            } else {
                None
            };
            fields.push(StructPatternField { name, pattern });
            if self.eat_kind(&TokenKind::RBrace) {
                break;
            }
            self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
        }
        Ok(fields)
    }

    /// Parses `pattern => expr` or `pattern => { block }`.
    pub(crate) fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let pattern = self.parse_pattern()?;
        let guard = if self.eat_keyword(Keyword::If) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect_kind(ExpectedToken::Punct("=>"), &TokenKind::FatArrow)?;
        let body = self.parse_expr_or_block_value()?;
        self.expect_semi()?;
        Ok(MatchArm {
            pattern,
            guard,
            body,
        })
    }

    /// Parses the right-hand side of a match arm (expression or braced block).
    pub(crate) fn parse_expr_or_block_value(&mut self) -> Result<ExprNode, ParseError> {
        if self.peek_kind() == TokenKind::LBrace {
            let block = self.parse_block()?;
            let span = block.span;
            return Ok(self.node(crate::ast::Expr::Block(block), span));
        }
        self.parse_expr()
    }
}

use phx_diagnostics::ParseError;

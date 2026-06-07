//! Parser for `#[...]` item attributes.

#![allow(clippy::elidable_lifetime_names)]

use phx_diagnostics::{ExpectedToken, ParseError};

use crate::ast::Node;
use crate::ast::attr::{AttrArg, AttrValue, Attribute};
use crate::ast::ident::Ident;
use crate::parser::Parser;
use crate::token::{Keyword, TokenKind};

impl<'src> Parser<'src> {
    /// Parses zero or more `#[ident(args?)]` attributes.
    pub(crate) fn parse_attribute_list(&mut self) -> Result<Vec<Node<Attribute>>, ParseError> {
        let mut attrs = Vec::new();
        while self.peek_kind() == TokenKind::HashBracket {
            attrs.push(self.parse_attribute()?);
        }
        Ok(attrs)
    }

    fn parse_attribute(&mut self) -> Result<Node<Attribute>, ParseError> {
        let start = self.pos;
        self.expect_kind(ExpectedToken::Punct("#["), &TokenKind::HashBracket)?;
        let name = self.parse_attr_name()?;
        let args = if self.eat_kind(&TokenKind::LParen) {
            self.parse_attr_args()?
        } else {
            Vec::new()
        };
        self.expect_kind(ExpectedToken::Punct("]"), &TokenKind::RBracket)?;
        Ok(self.node(Attribute { name, args }, self.span_from(start)))
    }

    fn parse_attr_name(&mut self) -> Result<Ident, ParseError> {
        match self.peek_kind() {
            TokenKind::Ident(text) => self.bump_ident(text),
            TokenKind::Keyword(Keyword::SelfLower) => {
                let span = self.current_span();
                self.bump();
                self.intern_ident("self", span)
            }
            _ => Err(self.error_unexpected(ExpectedToken::Ident)),
        }
    }

    fn parse_attr_args(&mut self) -> Result<Vec<AttrArg>, ParseError> {
        let mut args = Vec::new();
        if self.eat_kind(&TokenKind::RParen) {
            return Ok(args);
        }
        loop {
            args.push(self.parse_attr_arg()?);
            if self.eat_kind(&TokenKind::RParen) {
                break;
            }
            self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
        }
        Ok(args)
    }

    fn parse_attr_arg(&mut self) -> Result<AttrArg, ParseError> {
        if matches!(self.peek_kind(), TokenKind::TypeIdent(_)) {
            let ty = self.parse_type_name()?;
            return Ok(AttrArg::TypeName(ty));
        }
        let name = self.parse_attr_name()?;
        if self.eat_kind(&TokenKind::Eq) {
            let value = self.parse_attr_value()?;
            return Ok(AttrArg::Named { name, value });
        }
        if self.eat_kind(&TokenKind::LParen) {
            let args = self.parse_attr_args()?;
            return Ok(AttrArg::Nested { name, args });
        }
        Ok(AttrArg::Flag(name))
    }

    fn parse_attr_value(&mut self) -> Result<AttrValue, ParseError> {
        match self.peek_kind() {
            TokenKind::String(text) => {
                self.bump();
                Ok(AttrValue::Str(text.clone()))
            }
            TokenKind::Bool(b) => {
                self.bump();
                Ok(AttrValue::Bool(b))
            }
            TokenKind::Ident(text) => {
                let ident = self.bump_ident(text)?;
                Ok(AttrValue::Ident(ident))
            }
            _ => Err(self.error_unexpected(ExpectedToken::Token)),
        }
    }
}

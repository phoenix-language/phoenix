//! Type expression parsing.
//!
//! Handles primitives, named types, generics, references, pointers, tuples, arrays, slices, and
//! function types `:: (…) => T`.

use phx_diagnostics::{ExpectedToken, ParseError};

use crate::ast::types::{GenericParam, Type};
use crate::ast::{Node, TypeName};
use crate::parser::Parser;
use crate::token::{Keyword, TokenKind};

impl Parser<'_> {
    /// Parses a type expression (entry point for annotations and `as` casts).
    pub(crate) fn parse_type(&mut self) -> Result<Node<Type>, ParseError> {
        self.parse_type_expr()
    }

    fn parse_type_expr(&mut self) -> Result<Node<Type>, ParseError> {
        let start = self.pos;
        if self.eat_kind(&TokenKind::ColonColon) {
            self.expect_kind(ExpectedToken::Punct("("), &TokenKind::LParen)?;
            let params = self.parse_type_list()?;
            self.expect_fat_arrow()?;
            let ret = self.parse_type_expr()?;
            let span = self.span_from(start);
            return Ok(Node::new(
                Type::Function {
                    params,
                    ret: Box::new(ret),
                },
                span,
            ));
        }
        self.parse_type_postfix(start)
    }

    fn parse_type_postfix(&mut self, start: usize) -> Result<Node<Type>, ParseError> {
        let mut ty = self.parse_type_primary()?;
        if self.eat_kind(&TokenKind::Lt) {
            let args = self.parse_generic_args()?;
            let span = self.span_from(start);
            ty = match ty.inner {
                Type::Named {
                    name,
                    generics: None,
                } => Node::new(
                    Type::Named {
                        name,
                        generics: Some(args),
                    },
                    span,
                ),
                _ => return Err(self.error_unexpected(ExpectedToken::Type)),
            };
        }
        Ok(ty)
    }

    #[allow(clippy::too_many_lines)]
    fn parse_type_primary(&mut self) -> Result<Node<Type>, ParseError> {
        let start = self.pos;
        let kind = self.peek_kind();
        match kind {
            TokenKind::Keyword(k) if is_primitive_keyword(k) => {
                self.bump();
                Ok(Node::new(Type::Primitive(k), self.span_from(start)))
            }
            TokenKind::TypeIdent(name) => {
                let span = self.current_span();
                self.bump();
                Ok(Node::new(
                    Type::Named {
                        name: self.intern_type_name(name, span)?,
                        generics: None,
                    },
                    self.span_from(start),
                ))
            }
            TokenKind::Keyword(Keyword::SelfUpper) => {
                let span = self.current_span();
                self.bump();
                Ok(Node::new(
                    Type::Named {
                        name: self.intern_type_name("Self", span)?,
                        generics: None,
                    },
                    self.span_from(start),
                ))
            }
            TokenKind::Amp | TokenKind::AmpMut => {
                let mut_ = self.eat_kind(&TokenKind::AmpMut);
                if !mut_ {
                    self.eat_kind(&TokenKind::Amp);
                }
                let inner = self.parse_type_expr()?;
                Ok(Node::new(
                    Type::Ref {
                        mut_,
                        inner: Box::new(inner),
                    },
                    self.span_from(start),
                ))
            }
            TokenKind::Star | TokenKind::StarMut => {
                let mut_ = self.eat_kind(&TokenKind::StarMut);
                if !mut_ {
                    self.eat_kind(&TokenKind::Star);
                }
                let inner = self.parse_type_expr()?;
                Ok(Node::new(
                    Type::Ptr {
                        mut_,
                        inner: Box::new(inner),
                    },
                    self.span_from(start),
                ))
            }
            TokenKind::LParen => {
                self.bump();
                if self.eat_kind(&TokenKind::RParen) {
                    return Ok(Node::new(Type::Unit, self.span_from(start)));
                }
                let first = self.parse_type_expr()?;
                if self.eat_kind(&TokenKind::RParen) {
                    return Ok(first);
                }
                self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
                let mut elems = vec![first];
                loop {
                    elems.push(self.parse_type_expr()?);
                    if self.eat_kind(&TokenKind::RParen) {
                        break;
                    }
                    self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
                }
                Ok(Node::new(Type::Tuple(elems), self.span_from(start)))
            }
            TokenKind::LBracket => {
                self.bump();
                let elem = self.parse_type_expr()?;
                if self.eat_kind(&TokenKind::Semicolon) {
                    let len = self.parse_int_lit()?;
                    self.expect_kind(ExpectedToken::Punct("]"), &TokenKind::RBracket)?;
                    return Ok(Node::new(
                        Type::Array {
                            elem: Box::new(elem),
                            len,
                        },
                        self.span_from(start),
                    ));
                }
                self.expect_kind(ExpectedToken::Punct("]"), &TokenKind::RBracket)?;
                Ok(Node::new(
                    Type::Slice(Box::new(elem)),
                    self.span_from(start),
                ))
            }
            _ => Err(self.error_unexpected(ExpectedToken::Type)),
        }
    }

    fn parse_type_list(&mut self) -> Result<Vec<Node<Type>>, ParseError> {
        if self.eat_kind(&TokenKind::RParen) {
            return Ok(Vec::new());
        }
        let mut list = vec![self.parse_type_expr()?];
        while self.eat_kind(&TokenKind::Comma) {
            list.push(self.parse_type_expr()?);
        }
        self.expect_kind(ExpectedToken::Punct(")"), &TokenKind::RParen)?;
        Ok(list)
    }

    /// Parses `<T>` or `<T: Bound, …>` on declarations.
    pub(crate) fn parse_generic_params(&mut self) -> Result<Vec<GenericParam>, ParseError> {
        self.expect_kind(ExpectedToken::Punct("<"), &TokenKind::Lt)?;
        let mut params = vec![self.parse_generic_param()?];
        while self.eat_kind(&TokenKind::Comma) {
            params.push(self.parse_generic_param()?);
        }
        self.expect_kind(ExpectedToken::Punct(">"), &TokenKind::Gt)?;
        Ok(params)
    }

    fn parse_generic_param(&mut self) -> Result<GenericParam, ParseError> {
        let name = self.parse_ident()?;
        let bounds = if self.eat_kind(&TokenKind::Colon) {
            Some(self.parse_trait_bounds()?)
        } else {
            None
        };
        Ok(GenericParam { name, bounds })
    }

    fn parse_trait_bounds(&mut self) -> Result<Vec<TypeName>, ParseError> {
        let mut bounds = vec![self.parse_type_name()?];
        while self.eat_kind(&TokenKind::Plus) {
            bounds.push(self.parse_type_name()?);
        }
        Ok(bounds)
    }

    /// Parses comma-separated type arguments; the leading `<` must already be consumed.
    /// Parses `<T, …>` type arguments (opening `<` already consumed).
    pub(crate) fn parse_generic_args(&mut self) -> Result<Vec<Node<Type>>, ParseError> {
        let mut args = vec![self.parse_type_expr()?];
        while self.eat_kind(&TokenKind::Comma) {
            args.push(self.parse_type_expr()?);
        }
        self.expect_kind(ExpectedToken::Punct(">"), &TokenKind::Gt)?;
        Ok(args)
    }

    /// Parses an integer literal token (radix and suffix).
    pub(crate) fn parse_int_lit(&mut self) -> Result<crate::ast::IntLit, ParseError> {
        match self.peek_kind() {
            TokenKind::Integer { value, suffix } => {
                self.bump();
                Ok(crate::ast::IntLit { value, suffix })
            }
            _ => Err(self.error_unexpected(ExpectedToken::Literal)),
        }
    }

    fn expect_fat_arrow(&mut self) -> Result<(), ParseError> {
        if self.eat_kind(&TokenKind::FatArrow) {
            Ok(())
        } else {
            Err(self.error_unexpected(ExpectedToken::Punct("=>")))
        }
    }
}

fn is_primitive_keyword(k: Keyword) -> bool {
    matches!(
        k,
        Keyword::Bool
            | Keyword::S8
            | Keyword::S16
            | Keyword::S32
            | Keyword::S64
            | Keyword::S128
            | Keyword::U8
            | Keyword::U16
            | Keyword::U32
            | Keyword::U64
            | Keyword::U128
            | Keyword::F32
            | Keyword::F64
    )
}

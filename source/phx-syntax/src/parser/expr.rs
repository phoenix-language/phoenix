//! Expression parsing (precedence climbing).
//!
//! Precedence runs from low to high: assignment → logical → bitwise → arithmetic → unary →
//! postfix → primary. Path and struct literal parsing borrow identifier text from tokens as `&str`.

use phx_diagnostics::ExpectedToken;

use crate::ast::TypeName;
use crate::ast::decl::Param;
use crate::ast::expr::{
    AssignOp, BinOp, Expr, ExprNode, LambdaBody, PostfixOp, RuntimeDirectiveKind, StructFieldInit,
    UnaryOp,
};
use crate::ast::ident::{Ident, TypePathSegment};
use crate::ast::lit::{FloatLit, IntLit, Literal};
use crate::intern::impl_receiver_symbol;
use crate::parser::Parser;
use crate::token::{Keyword, TokenKind};

impl Parser<'_> {
    /// Parses an expression (assignment level and below).
    pub(crate) fn parse_expr(&mut self) -> Result<ExprNode, ParseError> {
        self.parse_cast_expr()
    }

    /// Parses assignment expressions with trailing `as Type` casts.
    fn parse_cast_expr(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        let mut expr = self.parse_assign_expr()?;
        while self.eat_keyword(Keyword::As) {
            let ty = self.parse_type()?;
            let span = self.span_from(start);
            expr = self.node(
                Expr::Cast {
                    expr: Box::new(expr),
                    ty: Box::new(ty),
                },
                span,
            );
        }
        Ok(expr)
    }

    /// Parses `=` / `+=` / … assignment (right-associative).
    fn parse_assign_expr(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        let left = self.parse_logical_or_expr()?;
        if let Some(op) = parse_assign_op(self.peek_kind()) {
            self.bump();
            let right = self.parse_assign_expr()?;
            let span = self.span_from(start);
            return Ok(self.node(
                Expr::Assign {
                    op,
                    target: Box::new(left),
                    value: Box::new(right),
                },
                span,
            ));
        }
        Ok(left)
    }

    // Precedence (low → high): `||`, `&&`, equality, relational, `|`, `^`, `&`, shifts, `+/-`, `*`, `**`.

    /// Parses left-associative `||`.
    pub(crate) fn parse_logical_or_expr(&mut self) -> Result<ExprNode, ParseError> {
        self.parse_binary_chain(Self::parse_logical_and_expr, BinOp::Or, &TokenKind::OrOr)
    }

    /// Parses left-associative `&&`.
    fn parse_logical_and_expr(&mut self) -> Result<ExprNode, ParseError> {
        self.parse_binary_chain(Self::parse_range_expr, BinOp::And, &TokenKind::AndAnd)
    }

    /// Parses `equality_expr` with optional `..` / `..=` range suffix.
    fn parse_range_expr(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        let left = self.parse_equality_expr()?;
        let inclusive = match self.peek_kind() {
            TokenKind::DotDot => {
                self.bump();
                false
            }
            TokenKind::DotDotEq => {
                self.bump();
                true
            }
            _ => return Ok(left),
        };
        let right = self.parse_equality_expr()?;
        Ok(self.node(
            Expr::Range {
                start: Box::new(left),
                end: Box::new(right),
                inclusive,
            },
            self.span_from(start),
        ))
    }

    pub(crate) fn parse_equality_expr(&mut self) -> Result<ExprNode, ParseError> {
        let mut left = self.parse_relational_expr()?;
        while matches!(self.peek_kind(), TokenKind::EqEq | TokenKind::Ne) {
            let Some(tok) = self.bump() else {
                return Err(self.error_unexpected(ExpectedToken::Token));
            };
            let op = match tok.kind {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::Ne => BinOp::Ne,
                _ => return Err(self.error_unexpected(ExpectedToken::Token)),
            };
            let right = self.parse_relational_expr()?;
            let span = left.span.merge(right.span);
            left = self.node(
                Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_relational_expr(&mut self) -> Result<ExprNode, ParseError> {
        let mut left = self.parse_bitwise_or_expr()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Le => BinOp::Le,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::Ge => BinOp::Ge,
                _ => break,
            };
            self.bump();
            let right = self.parse_bitwise_or_expr()?;
            let span = left.span.merge(right.span);
            left = self.node(
                Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_bitwise_or_expr(&mut self) -> Result<ExprNode, ParseError> {
        self.parse_binary_chain(Self::parse_bitwise_xor_expr, BinOp::BitOr, &TokenKind::Pipe)
    }

    fn parse_bitwise_xor_expr(&mut self) -> Result<ExprNode, ParseError> {
        self.parse_binary_chain(
            Self::parse_bitwise_and_expr,
            BinOp::BitXor,
            &TokenKind::Caret,
        )
    }

    fn parse_bitwise_and_expr(&mut self) -> Result<ExprNode, ParseError> {
        self.parse_binary_chain(Self::parse_shift_expr, BinOp::BitAnd, &TokenKind::Amp)
    }

    fn parse_shift_expr(&mut self) -> Result<ExprNode, ParseError> {
        let mut left = self.parse_additive_expr()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Shl => BinOp::Shl,
                TokenKind::Shr => BinOp::Shr,
                _ => break,
            };
            self.bump();
            let right = self.parse_additive_expr()?;
            let span = left.span.merge(right.span);
            left = self.node(
                Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_additive_expr(&mut self) -> Result<ExprNode, ParseError> {
        let mut left = self.parse_multiplicative_expr()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let right = self.parse_multiplicative_expr()?;
            let span = left.span.merge(right.span);
            left = self.node(
                Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_multiplicative_expr(&mut self) -> Result<ExprNode, ParseError> {
        let mut left = self.parse_power_expr()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.bump();
            let right = self.parse_power_expr()?;
            let span = left.span.merge(right.span);
            left = self.node(
                Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_power_expr(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        let base = self.parse_unary_expr()?;
        if self.eat_kind(&TokenKind::StarStar) {
            let exp = self.parse_unary_expr()?;
            let span = self.span_from(start);
            return Ok(self.node(
                Expr::Binary {
                    op: BinOp::Pow,
                    left: Box::new(base),
                    right: Box::new(exp),
                },
                span,
            ));
        }
        Ok(base)
    }

    fn parse_unary_expr(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        let op = match self.peek_kind() {
            TokenKind::Minus => Some(UnaryOp::Neg),
            TokenKind::Bang => Some(UnaryOp::Not),
            TokenKind::Tilde => Some(UnaryOp::BitNot),
            TokenKind::Star => Some(UnaryOp::Deref),
            TokenKind::Amp => Some(UnaryOp::Ref),
            TokenKind::AmpMut => Some(UnaryOp::RefMut),
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            let operand = self.parse_unary_expr()?;
            let span = self.span_from(start);
            return Ok(self.node(
                Expr::Unary {
                    op,
                    operand: Box::new(operand),
                },
                span,
            ));
        }
        self.parse_postfix_expr()
    }

    /// Parses primary plus `.field`, calls, indexing, and `?`.
    fn parse_postfix_expr(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        let base = self.parse_primary_expr()?;
        let mut ops = Vec::new();
        loop {
            match self.peek_kind() {
                TokenKind::Dot => {
                    self.bump();
                    let name = self.parse_tuple_field_name()?;
                    let generics = if self.eat_kind(&TokenKind::Lt) {
                        Some(self.parse_generic_args()?)
                    } else {
                        None
                    };
                    if self.eat_kind(&TokenKind::LParen) {
                        let args = self.parse_arg_list()?;
                        ops.push(PostfixOp::Method {
                            name,
                            generics,
                            args,
                        });
                    } else {
                        ops.push(PostfixOp::Field(name));
                    }
                }
                TokenKind::ColonColon if matches!(self.peek_at(1), TokenKind::Lt) => {
                    self.bump();
                    self.bump();
                    let generics = Some(self.parse_generic_args()?);
                    self.expect_kind(ExpectedToken::Punct("("), &TokenKind::LParen)?;
                    let args = self.parse_arg_list()?;
                    ops.push(PostfixOp::Call { generics, args });
                }
                TokenKind::LParen => {
                    self.bump();
                    let args = self.parse_arg_list()?;
                    ops.push(PostfixOp::Call {
                        generics: None,
                        args,
                    });
                }
                TokenKind::LBracket => {
                    self.bump();
                    let idx = self.parse_expr()?;
                    self.expect_kind(ExpectedToken::Punct("]"), &TokenKind::RBracket)?;
                    ops.push(PostfixOp::Index(idx));
                }
                TokenKind::Question => {
                    self.bump();
                    ops.push(PostfixOp::Try);
                }
                _ => break,
            }
        }
        if ops.is_empty() {
            return Ok(base);
        }
        Ok(self.node(
            Expr::Postfix {
                base: Box::new(base),
                ops,
            },
            self.span_from(start),
        ))
    }

    /// Parses literals, paths, blocks, `if`/`match`, and parenthesized forms.
    fn parse_primary_expr(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        match self.peek_kind() {
            TokenKind::Integer { .. }
            | TokenKind::Float { .. }
            | TokenKind::Bool(_)
            | TokenKind::ByteChar(_)
            | TokenKind::ByteString(_)
            | TokenKind::String(_) => {
                let lit = self.parse_literal()?;
                Ok(self.node(Expr::Literal(lit), self.span_from(start)))
            }
            TokenKind::Ident(name) => {
                if matches!(self.peek_at(1), TokenKind::ColonColon | TokenKind::LBrace) {
                    return self.parse_path_or_struct_literal();
                }
                let name = *name;
                let span = self.current_span();
                self.bump();
                let ident = self.intern_ident(name, span)?;
                Ok(self.node(Expr::Ident(ident), self.span_from(start)))
            }
            TokenKind::Keyword(Keyword::SelfLower) => {
                self.bump();
                let self_span = self.span_from(start);
                let self_id = self.alloc_node_id();
                Ok(self.node(
                    Expr::Ident(Ident {
                        symbol: impl_receiver_symbol(),
                        span: self_span,
                        id: self_id,
                    }),
                    self_span,
                ))
            }
            TokenKind::LBrace => {
                let block = self.parse_block()?;
                Ok(self.node(Expr::Block(block), self.span_from(start)))
            }
            TokenKind::Keyword(Keyword::If) => self.parse_if_expr(),
            TokenKind::Keyword(Keyword::Match) => self.parse_match_expr(),
            TokenKind::Keyword(Keyword::Unsafe) => {
                self.bump();
                let block = self.parse_block()?;
                Ok(self.node(Expr::Unsafe(block), self.span_from(start)))
            }
            TokenKind::AtSpawn | TokenKind::AtSend | TokenKind::AtReceive | TokenKind::AtReply => {
                self.parse_runtime_directive()
            }
            TokenKind::LParen => self.parse_paren_or_tuple_or_lambda(),
            TokenKind::LBracket => self.parse_array_literal(),
            TokenKind::TypeIdent(_) => self.parse_path_or_struct_literal(),
            _ => Err(self.error_unexpected(ExpectedToken::Expr)),
        }
    }

    fn parse_paren_or_tuple_or_lambda(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        self.bump();
        if self.eat_kind(&TokenKind::RParen) {
            if self.eat_kind(&TokenKind::FatArrow) {
                return self.parse_lambda_body(start, Vec::new());
            }
            return Ok(self.node(Expr::Tuple(Vec::new()), self.span_from(start)));
        }
        if self.lambda_params_start() {
            let params = self.parse_lambda_params()?;
            if self.eat_kind(&TokenKind::FatArrow) {
                return self.parse_lambda_body(start, params);
            }
            return Err(self.error_unexpected(ExpectedToken::Punct("=>")));
        }
        let first = self.parse_expr()?;
        if self.eat_kind(&TokenKind::RParen) {
            return Ok(first);
        }
        if !self.eat_kind(&TokenKind::Comma) {
            return Err(self
                .expect_kind(ExpectedToken::Punct(")"), &TokenKind::RParen)
                .err()
                .unwrap_or_else(|| ParseError::UnexpectedEof {
                    expected: ExpectedToken::Punct(")"),
                    span: self.current_span(),
                }));
        }
        let mut elems = vec![first];
        loop {
            elems.push(self.parse_expr()?);
            if self.eat_kind(&TokenKind::RParen) {
                break;
            }
            self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
        }
        Ok(self.node(Expr::Tuple(elems), self.span_from(start)))
    }

    fn parse_array_literal(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        self.bump();
        let mut elems = Vec::new();
        if !self.eat_kind(&TokenKind::RBracket) {
            elems.push(self.parse_expr()?);
            while self.eat_kind(&TokenKind::Comma) {
                elems.push(self.parse_expr()?);
            }
            self.expect_kind(ExpectedToken::Punct("]"), &TokenKind::RBracket)?;
        }
        Ok(self.node(Expr::Array(elems), self.span_from(start)))
    }

    /// Parses `Type { … }`, `a::b`, or a single-segment path/ident.
    #[allow(clippy::too_many_lines, clippy::collapsible_if)]
    fn parse_path_or_struct_literal(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        let (type_name, first_segment, from_type_ident) = match self.peek_kind() {
            TokenKind::TypeIdent(n) => {
                let n = *n;
                let span = self.current_span();
                self.bump();
                let tn = self.intern_type_name(n, span)?;
                (
                    tn,
                    crate::ast::PathSegment::Type(TypePathSegment::new(tn)),
                    true,
                )
            }
            TokenKind::Ident(n) => {
                let n = *n;
                let id = self.bump_ident(n)?;
                if matches!(self.peek_kind(), TokenKind::ColonColon)
                    && matches!(self.peek_at(1), TokenKind::Lt)
                {
                    let saved = self.pos;
                    self.bump();
                    self.bump();
                    if let Ok(generic_args) = self.parse_generic_args() {
                        if self.brace_starts_struct_literal_body(false) {
                            let type_name = TypeName {
                                symbol: id.symbol,
                                span: id.span,
                                id: id.id,
                            };
                            self.bump();
                            let fields = self.parse_struct_field_inits()?;
                            return Ok(self.node(
                                Expr::StructLit {
                                    name: type_name,
                                    generics: Some(generic_args),
                                    fields,
                                },
                                self.span_from(start),
                            ));
                        }
                    }
                    self.pos = saved;
                    return Ok(self.node(Expr::Ident(id), self.span_from(start)));
                }
                (
                    TypeName {
                        symbol: id.symbol,
                        span: id.span,
                        id: id.id,
                    },
                    crate::ast::PathSegment::Ident(id),
                    false,
                )
            }
            _ => return Err(self.error_unexpected(ExpectedToken::Ident)),
        };
        let generics = if from_type_ident {
            if matches!(self.peek_kind(), TokenKind::ColonColon)
                && matches!(self.peek_at(1), TokenKind::Lt)
            {
                self.bump();
                self.bump();
                Some(self.parse_generic_args()?)
            } else if self.eat_kind(&TokenKind::Lt) {
                Some(self.parse_generic_args()?)
            } else {
                None
            }
        } else {
            None
        };
        if self.brace_starts_struct_literal_body(from_type_ident) {
            self.bump();
            let fields = self.parse_struct_field_inits()?;
            return Ok(self.node(
                Expr::StructLit {
                    name: type_name,
                    generics,
                    fields,
                },
                self.span_from(start),
            ));
        }
        if generics.is_some() && matches!(self.peek_kind(), TokenKind::LParen) {
            self.bump();
            let args = self.parse_arg_list()?;
            let base = self.node(
                Expr::Path(crate::ast::Path {
                    segments: vec![first_segment],
                }),
                self.span_from(start),
            );
            return Ok(self.node(
                Expr::Postfix {
                    base: Box::new(base),
                    ops: vec![PostfixOp::Call { generics, args }],
                },
                self.span_from(start),
            ));
        }
        if self.eat_kind(&TokenKind::ColonColon) {
            let head = match (generics, first_segment) {
                (Some(type_args), crate::ast::PathSegment::Type(mut seg)) => {
                    seg.generics = Some(type_args);
                    crate::ast::PathSegment::Type(seg)
                }
                (_, seg) => seg,
            };
            let mut segments = vec![head];
            loop {
                match self.peek_kind() {
                    TokenKind::TypeIdent(seg) => {
                        let seg = *seg;
                        let span = self.current_span();
                        self.bump();
                        segments.push(crate::ast::PathSegment::Type(TypePathSegment::new(
                            self.intern_type_name(seg, span)?,
                        )));
                    }
                    TokenKind::Ident(seg) => {
                        let seg = *seg;
                        segments.push(crate::ast::PathSegment::Ident(self.bump_ident(seg)?));
                    }
                    _ => break,
                }
                if !self.eat_kind(&TokenKind::ColonColon) {
                    break;
                }
            }
            return Ok(self.node(
                Expr::Path(crate::ast::Path { segments }),
                self.span_from(start),
            ));
        }
        Ok(self.node(
            Expr::Path(crate::ast::Path {
                segments: vec![first_segment],
            }),
            self.span_from(start),
        ))
    }

    fn parse_struct_field_inits(&mut self) -> Result<Vec<StructFieldInit>, ParseError> {
        let mut fields = Vec::new();
        if self.eat_kind(&TokenKind::RBrace) {
            return Ok(fields);
        }
        loop {
            if self.eat_kind(&TokenKind::DotDot) {
                let base = self.parse_expr()?;
                fields.push(StructFieldInit::Spread(base));
            } else {
                let name = self.parse_ident()?;
                self.expect_kind(ExpectedToken::Punct(":"), &TokenKind::Colon)?;
                let value = self.parse_expr()?;
                fields.push(StructFieldInit::Field { name, value });
            }
            if self.eat_kind(&TokenKind::RBrace) {
                break;
            }
            self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
        }
        Ok(fields)
    }

    /// Parses a numeric, byte, float, bool, or unit literal.
    pub(crate) fn parse_literal(&mut self) -> Result<Literal, ParseError> {
        match self.peek_kind() {
            TokenKind::Integer { value, suffix } => {
                let value = *value;
                let suffix = *suffix;
                self.bump();
                Ok(Literal::Int(IntLit { value, suffix }))
            }
            TokenKind::Float { value, suffix } => {
                let value = *value;
                let suffix = *suffix;
                self.bump();
                Ok(Literal::Float(FloatLit { value, suffix }))
            }
            TokenKind::Bool(b) => {
                let b = *b;
                self.bump();
                Ok(Literal::Bool(b))
            }
            TokenKind::ByteChar(b) => {
                let b = *b;
                self.bump();
                Ok(Literal::ByteChar(b))
            }
            TokenKind::ByteString(_) => match self.bump_kind() {
                Some(TokenKind::ByteString(b)) => Ok(Literal::ByteString(b)),
                _ => Err(self.error_unexpected(ExpectedToken::Literal)),
            },
            TokenKind::String(_) => match self.bump_kind() {
                Some(TokenKind::String(s)) => Ok(Literal::String(s)),
                _ => Err(self.error_unexpected(ExpectedToken::Literal)),
            },
            _ => Err(self.error_unexpected(ExpectedToken::Literal)),
        }
    }

    fn parse_arg_list(&mut self) -> Result<Vec<ExprNode>, ParseError> {
        if self.eat_kind(&TokenKind::RParen) {
            return Ok(Vec::new());
        }
        let mut args = vec![self.parse_expr()?];
        while self.eat_kind(&TokenKind::Comma) {
            args.push(self.parse_expr()?);
        }
        self.expect_kind(ExpectedToken::Punct(")"), &TokenKind::RParen)?;
        Ok(args)
    }

    /// Parses `if` / `else if` / `else` as an expression.
    fn parse_if_expr(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        self.eat_keyword(Keyword::If);
        let condition = self.parse_if_condition()?;
        let then_block = self.parse_block()?;
        let mut else_ifs = Vec::new();
        while matches!(self.peek_kind(), TokenKind::Keyword(Keyword::Else))
            && matches!(self.peek_at(1), TokenKind::Keyword(Keyword::If))
        {
            self.eat_keyword(Keyword::Else);
            self.eat_keyword(Keyword::If);
            let econd = self.parse_if_condition()?;
            let eblock = self.parse_block()?;
            else_ifs.push((econd, eblock));
        }
        let else_block = if self.eat_keyword(Keyword::Else) {
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(self.node(
            Expr::If {
                condition: Box::new(condition),
                then_block,
                else_ifs,
                else_block,
            },
            self.span_from(start),
        ))
    }

    /// Parses a boolean condition or `const` / `var` pattern binding after `if`.
    fn parse_if_condition(&mut self) -> Result<crate::ast::expr::IfCondition, ParseError> {
        match self.peek_kind() {
            TokenKind::Keyword(Keyword::Const) => {
                self.bump();
                let pattern = self.parse_pattern()?;
                self.expect_kind(ExpectedToken::Punct("="), &TokenKind::Eq)?;
                let scrutinee = self.parse_expr()?;
                Ok(crate::ast::expr::IfCondition::Pattern {
                    mutable: false,
                    pattern,
                    scrutinee,
                })
            }
            TokenKind::Keyword(Keyword::Var) => {
                self.bump();
                let pattern = self.parse_pattern()?;
                self.expect_kind(ExpectedToken::Punct("="), &TokenKind::Eq)?;
                let scrutinee = self.parse_expr()?;
                Ok(crate::ast::expr::IfCondition::Pattern {
                    mutable: true,
                    pattern,
                    scrutinee,
                })
            }
            _ => {
                let cond = self.parse_logical_or_expr()?;
                Ok(crate::ast::expr::IfCondition::Bool(cond))
            }
        }
    }

    /// Parses `match scrutinee { arms… }`.
    fn parse_match_expr(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        self.eat_keyword(Keyword::Match);
        let scrutinee = self.parse_expr()?;
        self.expect_kind(ExpectedToken::Punct("{"), &TokenKind::LBrace)?;
        let mut arms = Vec::new();
        while !self.eat_kind(&TokenKind::RBrace) {
            arms.push(self.parse_match_arm()?);
        }
        Ok(self.node(
            Expr::Match {
                scrutinee: Box::new(scrutinee),
                arms,
            },
            self.span_from(start),
        ))
    }

    fn parse_binary_chain<F>(
        &mut self,
        mut next: F,
        op: BinOp,
        token: &TokenKind<'_>,
    ) -> Result<ExprNode, ParseError>
    where
        F: FnMut(&mut Self) -> Result<ExprNode, ParseError>,
    {
        let mut left = next(self)?;
        while self.peek_kind() == token {
            self.bump();
            let right = next(self)?;
            let span = left.span.merge(right.span);
            left = self.node(
                Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    /// Returns `true` when `(` is followed by lambda parameter syntax.
    fn lambda_params_start(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(_))
            && matches!(self.peek_at(1), TokenKind::Colon)
            || matches!(
                self.peek_kind(),
                TokenKind::Keyword(Keyword::Mut | Keyword::SelfLower)
            )
    }

    /// Parses lambda parameters after `(` (opening paren already consumed).
    fn parse_lambda_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        loop {
            params.push(self.parse_param()?);
            if self.eat_kind(&TokenKind::RParen) {
                break;
            }
            self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
        }
        Ok(params)
    }

    /// Parses `=>` body for a lambda with `params` already parsed.
    fn parse_lambda_body(
        &mut self,
        start: usize,
        params: Vec<Param>,
    ) -> Result<ExprNode, ParseError> {
        let body = if matches!(self.peek_kind(), TokenKind::LBrace) {
            LambdaBody::Block(self.parse_block()?)
        } else {
            LambdaBody::Expr(Box::new(self.parse_expr()?))
        };
        Ok(self.node(Expr::Lambda { params, body }, self.span_from(start)))
    }

    /// Parses `@spawn` / `@send` / `@receive` / `@reply` primary expressions.
    fn parse_runtime_directive(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        let kind = match self.peek_kind() {
            TokenKind::AtSpawn => RuntimeDirectiveKind::Spawn,
            TokenKind::AtSend => RuntimeDirectiveKind::Send,
            TokenKind::AtReceive => RuntimeDirectiveKind::Receive,
            TokenKind::AtReply => RuntimeDirectiveKind::Reply,
            _ => return Err(self.error_unexpected(ExpectedToken::Expr)),
        };
        self.bump();
        self.expect_kind(ExpectedToken::Punct("("), &TokenKind::LParen)?;
        let args = match kind {
            RuntimeDirectiveKind::Spawn
            | RuntimeDirectiveKind::Receive
            | RuntimeDirectiveKind::Reply => {
                let arg = self.parse_expr()?;
                self.expect_kind(ExpectedToken::Punct(")"), &TokenKind::RParen)?;
                vec![arg]
            }
            RuntimeDirectiveKind::Send => {
                let a = self.parse_expr()?;
                self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
                let b = self.parse_expr()?;
                self.expect_kind(ExpectedToken::Punct(")"), &TokenKind::RParen)?;
                vec![a, b]
            }
        };
        Ok(self.node(Expr::RuntimeDirective { kind, args }, self.span_from(start)))
    }
}

use phx_diagnostics::ParseError;

fn parse_assign_op(kind: &TokenKind<'_>) -> Option<AssignOp> {
    match *kind {
        TokenKind::Eq => Some(AssignOp::Assign),
        TokenKind::PlusEq => Some(AssignOp::AddAssign),
        TokenKind::MinusEq => Some(AssignOp::SubAssign),
        TokenKind::StarEq => Some(AssignOp::MulAssign),
        TokenKind::SlashEq => Some(AssignOp::DivAssign),
        TokenKind::PercentEq => Some(AssignOp::ModAssign),
        _ => None,
    }
}

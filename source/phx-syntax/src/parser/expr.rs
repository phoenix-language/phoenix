//! Expression parsing (precedence climbing).
//!
//! Precedence runs from low to high: assignment → logical → bitwise → arithmetic → unary →
//! postfix → primary. Path and struct literal parsing borrow identifier text from tokens as `&str`.

use phx_diagnostics::ExpectedToken;

use crate::ast::Node;
use crate::ast::TypeName;
use crate::ast::expr::{AssignOp, BinOp, Expr, ExprNode, PostfixOp, StructFieldInit, UnaryOp};
use crate::ast::ident::Ident;
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
            expr = Node::new(
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
        let kind = self.peek_kind();
        if let Some(op) = parse_assign_op(&kind) {
            self.bump();
            let right = self.parse_assign_expr()?;
            let span = self.span_from(start);
            return Ok(Node::new(
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
    fn parse_logical_or_expr(&mut self) -> Result<ExprNode, ParseError> {
        self.parse_binary_chain(Self::parse_logical_and_expr, BinOp::Or, &TokenKind::OrOr)
    }

    /// Parses left-associative `&&`.
    fn parse_logical_and_expr(&mut self) -> Result<ExprNode, ParseError> {
        self.parse_binary_chain(Self::parse_range_expr, BinOp::And, &TokenKind::AndAnd)
    }

    /// Parses relational/equality level; rejects `..` range syntax (post-MVP).
    fn parse_range_expr(&mut self) -> Result<ExprNode, ParseError> {
        let expr = self.parse_equality_expr()?;
        if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotEq) {
            return Err(self.reject_unsupported("range expression"));
        }
        Ok(expr)
    }

    fn parse_equality_expr(&mut self) -> Result<ExprNode, ParseError> {
        let mut left = self.parse_relational_expr()?;
        while matches!(self.peek_kind(), TokenKind::EqEq | TokenKind::Ne) {
            let Some(tok) = self.bump() else {
                return Err(self.error_unexpected(ExpectedToken::Token));
            };
            let op = match tok.kind {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::Ne => BinOp::Ne,
                _ => unreachable!(),
            };
            let right = self.parse_relational_expr()?;
            let span = left.span.merge(right.span);
            left = Node::new(
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
            left = Node::new(
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
            left = Node::new(
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
            left = Node::new(
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
            left = Node::new(
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
            return Ok(Node::new(
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
            return Ok(Node::new(
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
                    let name = self.parse_ident()?;
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
                TokenKind::LParen => {
                    self.bump();
                    let args = self.parse_arg_list()?;
                    ops.push(PostfixOp::Call(args));
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
                TokenKind::AtSpawn
                | TokenKind::AtSend
                | TokenKind::AtReceive
                | TokenKind::AtReply => {
                    return Err(self.reject_deferred_directive());
                }
                _ => break,
            }
        }
        if ops.is_empty() {
            return Ok(base);
        }
        Ok(Node::new(
            Expr::Postfix {
                base: Box::new(base),
                ops,
            },
            self.span_from(start),
        ))
    }

    /// Parses literals, paths, blocks, `if`/`match`, and parenthesized forms.
    fn parse_primary_expr(&mut self) -> Result<ExprNode, ParseError> {
        if matches!(
            self.peek_kind(),
            TokenKind::AtSpawn | TokenKind::AtSend | TokenKind::AtReceive | TokenKind::AtReply
        ) {
            return Err(self.reject_deferred_directive());
        }
        let start = self.pos;
        match self.peek_kind() {
            TokenKind::Integer { .. }
            | TokenKind::Float { .. }
            | TokenKind::Bool(_)
            | TokenKind::ByteChar(_)
            | TokenKind::ByteString(_) => {
                let lit = self.parse_literal()?;
                Ok(Node::new(Expr::Literal(lit), self.span_from(start)))
            }
            TokenKind::Ident(name) => {
                if matches!(
                    self.peek_at(1),
                    TokenKind::ColonColon | TokenKind::LBrace
                ) {
                    return self.parse_path_or_struct_literal();
                }
                self.bump();
                Ok(Node::new(
                    Expr::Ident(self.intern_ident(name)),
                    self.span_from(start),
                ))
            }
            TokenKind::Keyword(Keyword::SelfLower) => {
                self.bump();
                Ok(Node::new(
                    Expr::Ident(Ident {
                        symbol: impl_receiver_symbol(),
                    }),
                    self.span_from(start),
                ))
            }
            TokenKind::LBrace => {
                let block = self.parse_block()?;
                Ok(Node::new(Expr::Block(block), self.span_from(start)))
            }
            TokenKind::Keyword(Keyword::If) => self.parse_if_expr(),
            TokenKind::Keyword(Keyword::Match) => self.parse_match_expr(),
            TokenKind::HashUnsafe => {
                self.bump();
                let block = self.parse_block()?;
                Ok(Node::new(Expr::Unsafe(block), self.span_from(start)))
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
                return Err(self.reject_unsupported("lambda expression"));
            }
            return Ok(Node::new(Expr::Tuple(Vec::new()), self.span_from(start)));
        }
        let first = self.parse_expr()?;
        if self.eat_kind(&TokenKind::RParen) {
            if self.eat_kind(&TokenKind::FatArrow) {
                return Err(self.reject_unsupported("lambda expression"));
            }
            return Ok(first);
        }
        self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
        let mut elems = vec![first];
        loop {
            elems.push(self.parse_expr()?);
            if self.eat_kind(&TokenKind::RParen) {
                break;
            }
            self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
        }
        Ok(Node::new(Expr::Tuple(elems), self.span_from(start)))
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
        Ok(Node::new(Expr::Array(elems), self.span_from(start)))
    }

    /// Parses `Type { … }`, `a::b`, or a single-segment path/ident.
    fn parse_path_or_struct_literal(&mut self) -> Result<ExprNode, ParseError> {
        let start = self.pos;
        let (type_name, first_segment, from_type_ident) = match self.peek_kind() {
            TokenKind::TypeIdent(n) => {
                self.bump();
                let tn = self.intern_type_name(n);
                (tn, crate::ast::PathSegment::Type(tn), true)
            }
            TokenKind::Ident(n) => {
                self.bump();
                let id = self.intern_ident(n);
                (
                    TypeName { symbol: id.symbol },
                    crate::ast::PathSegment::Ident(id),
                    false,
                )
            }
            _ => return Err(self.error_unexpected(ExpectedToken::Ident)),
        };
        if from_type_ident && self.eat_kind(&TokenKind::Lt) {
            let _generics = self.parse_generic_args()?;
        }
        if self.brace_starts_struct_literal_body(from_type_ident) {
            self.bump();
            let fields = self.parse_struct_field_inits()?;
            return Ok(Node::new(
                Expr::StructLit {
                    name: type_name,
                    generics: None,
                    fields,
                },
                self.span_from(start),
            ));
        }
        if self.eat_kind(&TokenKind::ColonColon) {
            let mut segments = vec![first_segment];
            loop {
                match self.peek_kind() {
                    TokenKind::TypeIdent(seg) => {
                        self.bump();
                        segments.push(crate::ast::PathSegment::Type(self.intern_type_name(seg)));
                    }
                    TokenKind::Ident(seg) => {
                        self.bump();
                        segments.push(crate::ast::PathSegment::Ident(self.intern_ident(seg)));
                    }
                    _ => break,
                }
                if !self.eat_kind(&TokenKind::ColonColon) {
                    break;
                }
            }
            return Ok(Node::new(
                Expr::Path(crate::ast::Path { segments }),
                self.span_from(start),
            ));
        }
        Ok(Node::new(
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

    pub(crate) fn parse_literal(&mut self) -> Result<Literal, ParseError> {
        match self.peek_kind() {
            TokenKind::Integer { value, suffix } => {
                self.bump();
                Ok(Literal::Int(IntLit { value, suffix }))
            }
            TokenKind::Float { value, suffix } => {
                self.bump();
                Ok(Literal::Float(FloatLit { value, suffix }))
            }
            TokenKind::Bool(b) => {
                self.bump();
                Ok(Literal::Bool(b))
            }
            TokenKind::ByteChar(b) => {
                self.bump();
                Ok(Literal::ByteChar(b))
            }
            TokenKind::ByteString(b) => {
                self.bump();
                Ok(Literal::ByteString(b))
            }
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
        // Condition must not include trailing `{ … }` blocks or `else`; use logical level only.
        let cond = self.parse_logical_or_expr()?;
        let then_block = self.parse_block()?;
        let mut else_ifs = Vec::new();
        while matches!(self.peek_kind(), TokenKind::Keyword(Keyword::Else))
            && matches!(self.peek_at(1), TokenKind::Keyword(Keyword::If))
        {
            self.eat_keyword(Keyword::Else);
            self.eat_keyword(Keyword::If);
            let econd = self.parse_logical_or_expr()?;
            let eblock = self.parse_block()?;
            else_ifs.push((econd, eblock));
        }
        let else_block = if self.eat_keyword(Keyword::Else) {
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Node::new(
            Expr::If {
                cond: Box::new(cond),
                then_block,
                else_ifs,
                else_block,
            },
            self.span_from(start),
        ))
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
        Ok(Node::new(
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
        while self.peek_kind() == *token {
            self.bump();
            let right = next(self)?;
            let span = left.span.merge(right.span);
            left = Node::new(
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

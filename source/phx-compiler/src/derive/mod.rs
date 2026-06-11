//! `#derive(...)` expansion before name resolution.
//!
//! Synthesizes trait `impl` items for supported derives on structs and enums.

use phx_diagnostics::Span;
use phx_syntax::ast::decl::{
    DeriveDirective, EnumVariant, Function, ImplMember, Param, StructBody, TopLevelDecl,
    TopLevelItem, Variant,
};
use phx_syntax::ast::expr::{BinOp, Expr, IfCondition, PostfixOp, UnaryOp};
use phx_syntax::ast::ident::{Ident, TypeName};
use phx_syntax::ast::lit::{IntLit, Literal};
use phx_syntax::ast::pat::MatchArm;
use phx_syntax::ast::pat::{Pattern, StructPatternField};
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};
use phx_syntax::ast::types::{GenericParam, Type};
use phx_syntax::ast::{AstNodeId, BlockNode, ExprNode, Node, PatternNode};
use phx_syntax::token::{IntegerSuffix, Keyword};
use phx_syntax::{Interner, Program, Symbol, impl_receiver_symbol};

/// Failure while expanding `#derive(...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeriveError {
    /// Related source span.
    pub span: Span,
    /// Human-readable message.
    pub message: String,
}

const SUPPORTED_DERIVES: &[&str] = &["Copyable", "PartialEq", "Debug"];

/// Supported compile-time derive traits (V0-056).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeriveTrait {
    Copyable,
    PartialEq,
    Debug,
}

impl DeriveTrait {
    fn parse(interner: &Interner, name: Symbol) -> Option<Self> {
        match interner.resolve(name) {
            "Copyable" => Some(Self::Copyable),
            "PartialEq" => Some(Self::PartialEq),
            "Debug" => Some(Self::Debug),
            _ => None,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Copyable => "Copyable",
            Self::PartialEq => "PartialEq",
            Self::Debug => "Debug",
        }
    }
}

/// Expands `#derive` / `#[derive]` on structs and enums into synthetic trait impl items.
///
/// # Errors
///
/// Returns [`DeriveError`] for unsupported traits, generics, or duplicate impls.
pub fn expand_derives(program: &mut Program, interner: &Interner) -> Result<(), DeriveError> {
    let existing = collect_existing_impls(&program.items, interner);
    let taken = std::mem::take(&mut program.items);
    let mut out = Vec::with_capacity(taken.len());
    for mut item in taken {
        let extra = match &mut item.inner.decl {
            TopLevelDecl::Struct {
                name,
                derives,
                generics,
                body,
            } if !derives.is_empty() => {
                let span = item.span;
                let expanded = expand_type_derives(
                    interner,
                    &existing,
                    name,
                    generics.as_deref(),
                    TypeShape::Struct(body),
                    derives,
                    span,
                )?;
                *derives = Vec::new();
                expanded
            }
            TopLevelDecl::Enum {
                name,
                derives,
                generics,
                variants,
            } if !derives.is_empty() => {
                let span = item.span;
                let expanded = expand_type_derives(
                    interner,
                    &existing,
                    name,
                    generics.as_deref(),
                    TypeShape::Enum(variants),
                    derives,
                    span,
                )?;
                *derives = Vec::new();
                expanded
            }
            _ => Vec::new(),
        };
        out.push(item);
        out.extend(extra);
    }
    program.items = out;
    Ok(())
}

#[derive(Clone, Copy)]
enum TypeShape<'a> {
    Struct(&'a StructBody),
    Enum(&'a [EnumVariant]),
}

fn collect_existing_impls(
    items: &[Node<TopLevelItem>],
    interner: &Interner,
) -> Vec<(Symbol, Symbol)> {
    let mut out = Vec::new();
    for item in items {
        if let TopLevelDecl::Impl {
            type_name, trait_, ..
        } = &item.inner.decl
            && let Some(trait_sym) = trait_symbol(trait_.as_ref())
        {
            out.push((type_name.symbol, trait_sym));
        }
    }
    let _ = interner;
    out
}

fn trait_symbol(trait_: Option<&Node<Type>>) -> Option<Symbol> {
    match trait_.map(|t| &t.inner) {
        Some(Type::Named { name, .. }) => Some(name.symbol),
        _ => None,
    }
}

fn expand_type_derives(
    interner: &Interner,
    existing: &[(Symbol, Symbol)],
    type_name: &TypeName,
    generics: Option<&[GenericParam]>,
    shape: TypeShape<'_>,
    derives: &[DeriveDirective],
    span: Span,
) -> Result<Vec<Node<TopLevelItem>>, DeriveError> {
    if generics.is_some_and(|g| !g.is_empty()) {
        return Err(DeriveError {
            span,
            message: "derive is not supported on generic types yet".to_string(),
        });
    }
    let mut builder = AstGen::new(interner, span);
    let mut seen = Vec::new();
    let mut impls = Vec::new();
    for dir in derives {
        for trait_name in &dir.traits {
            let Some(kind) = DeriveTrait::parse(interner, trait_name.symbol) else {
                let supported = SUPPORTED_DERIVES.join(", ");
                let name = interner.resolve(trait_name.symbol);
                return Err(DeriveError {
                    span: trait_name.span,
                    message: format!("unsupported derive trait `{name}`; supported: {supported}"),
                });
            };
            if seen.contains(&kind) {
                return Err(DeriveError {
                    span: trait_name.span,
                    message: format!(
                        "duplicate derive `{}` for `{}`",
                        kind.as_str(),
                        interner.resolve(type_name.symbol)
                    ),
                });
            }
            if existing
                .iter()
                .any(|(t, tr)| *t == type_name.symbol && interner.resolve(*tr) == kind.as_str())
                || impls
                    .iter()
                    .any(|item| impl_trait_kind(item, interner) == Some(kind))
            {
                return Err(DeriveError {
                    span: trait_name.span,
                    message: format!(
                        "cannot derive {}: `{}` already implements {}",
                        kind.as_str(),
                        interner.resolve(type_name.symbol),
                        kind.as_str()
                    ),
                });
            }
            seen.push(kind);
            let item = match kind {
                DeriveTrait::Copyable => builder.copyable_impl(type_name),
                DeriveTrait::PartialEq => builder.partialeq_impl(type_name, shape),
                DeriveTrait::Debug => builder.debug_impl(type_name, interner),
            };
            impls.push(item);
        }
    }
    Ok(impls)
}

fn impl_trait_kind(item: &Node<TopLevelItem>, interner: &Interner) -> Option<DeriveTrait> {
    let TopLevelDecl::Impl { trait_, .. } = &item.inner.decl else {
        return None;
    };
    trait_symbol(trait_.as_ref()).and_then(|s| DeriveTrait::parse(interner, s))
}

struct AstGen {
    interner: Interner,
    span: Span,
    next_id: u32,
}

impl AstGen {
    fn new(interner: &Interner, span: Span) -> Self {
        Self {
            interner: interner.clone(),
            span,
            next_id: 0x4_000_000,
        }
    }

    fn alloc_id(&mut self) -> AstNodeId {
        let id = self.next_id;
        self.next_id = id.saturating_add(1);
        AstNodeId::from_raw(id)
    }

    fn node<T>(&mut self, inner: T) -> Node<T> {
        Node::new(inner, self.span, self.alloc_id())
    }

    fn ident(&mut self, name: &str) -> Ident {
        let symbol = self.intern_sym(name);
        Ident {
            symbol,
            span: self.span,
            id: self.alloc_id(),
        }
    }

    fn type_name(&mut self, name: &str) -> TypeName {
        let symbol = self.intern_sym(name);
        TypeName {
            symbol,
            span: self.span,
            id: self.alloc_id(),
        }
    }

    #[allow(clippy::expect_used)]
    fn intern_sym(&mut self, name: &str) -> Symbol {
        self.interner
            .intern(name)
            .expect("derive: intern table should not be full during expansion")
    }

    fn type_bool(&mut self) -> Node<Type> {
        self.node(Type::Primitive(Keyword::Bool))
    }

    fn type_ref_self(&mut self) -> Node<Type> {
        let inner = self.type_self();
        self.node(Type::Ref {
            mut_: false,
            inner: Box::new(inner),
        })
    }

    fn type_self(&mut self) -> Node<Type> {
        let name = self.type_name("Self");
        self.node(Type::Named {
            name,
            generics: None,
        })
    }

    fn type_u8_array_32(&mut self) -> Node<Type> {
        let elem = self.node(Type::Primitive(Keyword::U8));
        self.node(Type::Array {
            elem: Box::new(elem),
            len: IntLit {
                value: 32,
                suffix: IntegerSuffix::None,
            },
        })
    }

    fn receiver_param(&mut self) -> Param {
        Param::Receiver {
            mut_: false,
            ty: Some(self.type_ref_self()),
        }
    }

    fn named_ref_self_param(&mut self, name: &str) -> Param {
        let param_name = self.ident(name);
        let ty = self.type_ref_self();
        Param::Named {
            name: param_name,
            ty,
        }
    }

    fn trait_type(&mut self, name: &str) -> Node<Type> {
        let trait_name = self.type_name(name);
        self.node(Type::Named {
            name: trait_name,
            generics: None,
        })
    }

    fn copyable_impl(&mut self, type_name: &TypeName) -> Node<TopLevelItem> {
        self.trait_impl(type_name, "Copyable", Vec::new())
    }

    fn partialeq_impl(&mut self, type_name: &TypeName, shape: TypeShape<'_>) -> Node<TopLevelItem> {
        let body = match shape {
            TypeShape::Struct(body) => self.struct_partialeq_body(type_name, body),
            TypeShape::Enum(variants) => self.enum_partialeq_body(type_name, variants),
        };
        let eq_params = vec![self.receiver_param(), self.named_ref_self_param("other")];
        let eq_ret = self.type_bool();
        let method = self.trait_method("eq", eq_params, eq_ret, body);
        self.trait_impl(type_name, "PartialEq", vec![ImplMember::Method(method)])
    }

    fn debug_impl(&mut self, type_name: &TypeName, interner: &Interner) -> Node<TopLevelItem> {
        let name = interner.resolve(type_name.symbol);
        let bytes = debug_name_bytes(name);
        let array_expr = self.u8_array_literal(&bytes);
        let block = self.expr_block(array_expr);
        let fmt_params = vec![self.receiver_param()];
        let fmt_ret = self.type_u8_array_32();
        let method = self.trait_method("fmt", fmt_params, fmt_ret, block);
        self.trait_impl(type_name, "Debug", vec![ImplMember::Method(method)])
    }

    fn trait_impl(
        &mut self,
        type_name: &TypeName,
        trait_name: &str,
        members: Vec<ImplMember>,
    ) -> Node<TopLevelItem> {
        let type_copy = TypeName {
            symbol: type_name.symbol,
            span: type_name.span,
            id: self.alloc_id(),
        };
        let trait_ty = self.trait_type(trait_name);
        self.node(TopLevelItem {
            attrs: Vec::new(),
            pub_: false,
            decl: TopLevelDecl::Impl {
                type_name: type_copy,
                generics: None,
                unsafe_: false,
                trait_: Some(trait_ty),
                members,
            },
        })
    }

    fn trait_method(
        &mut self,
        name: &str,
        params: Vec<Param>,
        ret: Node<Type>,
        body: BlockNode,
    ) -> Function {
        Function {
            attrs: Vec::new(),
            derives: Vec::new(),
            directives: Vec::new(),
            unsafe_: false,
            name: self.ident(name),
            generics: None,
            params,
            ret: Some(ret),
            body,
        }
    }

    fn expr_block(&mut self, tail: ExprNode) -> BlockNode {
        self.node(Block {
            items: vec![BlockItem::Expr(tail)],
        })
    }

    fn enum_rhs_if_const(
        &mut self,
        type_name: &TypeName,
        variant: &EnumVariant,
        idx: usize,
        lhs_bindings: &[Ident],
        rhs: &Ident,
    ) -> ExprNode {
        let (rhs_pat, rhs_bindings) = self.variant_pattern(type_name, variant, "b", idx);
        let then_body = self.variant_payload_eq(variant, lhs_bindings, &rhs_bindings);
        let then_block = self.expr_block(then_body);
        let else_tail = self.bool_lit(false);
        let else_block = self.expr_block(else_tail);
        let rhs_scrutinee = self.ident_expr(rhs);
        self.node(Expr::If {
            condition: Box::new(IfCondition::Pattern {
                mutable: false,
                pattern: rhs_pat,
                scrutinee: rhs_scrutinee,
            }),
            then_block,
            else_ifs: Vec::new(),
            else_block: Some(else_block),
        })
    }

    fn struct_partialeq_body(&mut self, type_name: &TypeName, body: &StructBody) -> BlockNode {
        let _ = type_name;
        let expr = match body {
            StructBody::Fields(fields) => {
                let comps: Vec<ExprNode> = fields
                    .iter()
                    .map(|f| self.field_eq(&f.name, &f.name))
                    .collect();
                self.and_chain_or_true(comps)
            }
            StructBody::Tuple(types) => {
                let comps: Vec<ExprNode> = (0..types.len())
                    .map(|i| {
                        let field = self.ident(&i.to_string());
                        self.field_eq(&field, &field)
                    })
                    .collect();
                self.and_chain_or_true(comps)
            }
            _ => self.bool_lit(true),
        };
        self.expr_block(expr)
    }

    fn enum_partialeq_body(&mut self, type_name: &TypeName, variants: &[EnumVariant]) -> BlockNode {
        let lhs = self.ident("__derived_lhs");
        let rhs = self.ident("__derived_rhs");
        let ty = self.type_named(type_name);
        let mut lhs_arms = Vec::with_capacity(variants.len());
        for (i, v) in variants.iter().enumerate() {
            let (lhs_pat, lhs_bindings) = self.variant_pattern(type_name, v, "a", i);
            let body = self.enum_rhs_if_const(type_name, v, i, &lhs_bindings, &rhs);
            lhs_arms.push(MatchArm {
                pattern: lhs_pat,
                guard: None,
                body,
            });
        }
        let lhs_scrutinee = self.ident_expr(&lhs);
        let match_tail = self.match_expr(lhs_scrutinee, lhs_arms);
        let self_base = self.self_expr();
        let lhs_init = self.deref_expr(self_base);
        let other_base = self.other_expr();
        let rhs_init = self.deref_expr(other_base);
        self.node(Block {
            items: vec![
                BlockItem::Stmt(Stmt::Const {
                    name: lhs,
                    ty: Some(ty.clone()),
                    init: lhs_init,
                }),
                BlockItem::Stmt(Stmt::Const {
                    name: rhs,
                    ty: Some(ty),
                    init: rhs_init,
                }),
                BlockItem::Expr(match_tail),
            ],
        })
    }

    fn type_named(&mut self, type_name: &TypeName) -> Node<Type> {
        let name_id = self.alloc_id();
        self.node(Type::Named {
            name: TypeName {
                symbol: type_name.symbol,
                span: type_name.span,
                id: name_id,
            },
            generics: None,
        })
    }

    fn variant_pattern(
        &mut self,
        _type_name: &TypeName,
        variant: &EnumVariant,
        prefix: &str,
        idx: usize,
    ) -> (PatternNode, Vec<Ident>) {
        let name = TypeName {
            symbol: variant.name.symbol,
            span: variant.name.span,
            id: self.alloc_id(),
        };
        match &variant.kind {
            Variant::Unit => {
                let id = self.alloc_id();
                let pat = self.node(Pattern::Ident(Ident {
                    symbol: variant.name.symbol,
                    span: variant.name.span,
                    id,
                }));
                (pat, Vec::new())
            }
            Variant::Tuple(types) => {
                let mut bindings = Vec::with_capacity(types.len());
                for f in 0..types.len() {
                    bindings.push(self.ident(&format!("__d_{prefix}_{idx}_{f}")));
                }
                let mut pats = Vec::with_capacity(bindings.len());
                for b in &bindings {
                    pats.push(self.node(Pattern::Ident(*b)));
                }
                let pat = self.node(Pattern::Tuple {
                    name,
                    patterns: pats,
                });
                (pat, bindings)
            }
            Variant::Struct(fields) => {
                let bindings: Vec<Ident> = fields
                    .iter()
                    .enumerate()
                    .map(|(f, _)| self.ident(&format!("__d_{prefix}_{idx}_{f}")))
                    .collect();
                let mut pat_fields = Vec::with_capacity(fields.len());
                for (field, bind) in fields.iter().zip(bindings.iter()) {
                    let field_id = self.alloc_id();
                    let bind_pat = self.node(Pattern::Ident(*bind));
                    pat_fields.push(StructPatternField {
                        name: Ident {
                            symbol: field.name.symbol,
                            span: field.name.span,
                            id: field_id,
                        },
                        pattern: Some(Box::new(bind_pat)),
                    });
                }
                let pat = self.node(Pattern::Struct {
                    name,
                    fields: pat_fields,
                });
                (pat, bindings)
            }
            _ => {
                let pat = self.node(Pattern::Wildcard);
                (pat, Vec::new())
            }
        }
    }

    fn variant_payload_eq(
        &mut self,
        variant: &EnumVariant,
        self_bindings: &[Ident],
        other_bindings: &[Ident],
    ) -> ExprNode {
        match &variant.kind {
            Variant::Tuple(_) | Variant::Struct(_) => {
                let comps: Vec<ExprNode> = self_bindings
                    .iter()
                    .zip(other_bindings.iter())
                    .map(|(a, b)| self.ident_eq(a, b))
                    .collect();
                self.and_chain_or_true(comps)
            }
            _ => self.bool_lit(true),
        }
    }

    fn and_chain_or_true(&mut self, exprs: Vec<ExprNode>) -> ExprNode {
        match self.and_chain(exprs) {
            Some(expr) => expr,
            None => self.bool_lit(true),
        }
    }

    fn field_eq(&mut self, field: &Ident, other_field: &Ident) -> ExprNode {
        let self_base = self.self_expr();
        let other_base = self.other_expr();
        let lhs = self.field_access(self_base, field);
        let rhs = self.field_access(other_base, other_field);
        self.bin_eq(lhs, rhs)
    }

    fn ident_eq(&mut self, a: &Ident, b: &Ident) -> ExprNode {
        let lhs = self.ident_expr(a);
        let rhs = self.ident_expr(b);
        self.bin_eq(lhs, rhs)
    }

    fn and_chain(&mut self, mut exprs: Vec<ExprNode>) -> Option<ExprNode> {
        if exprs.is_empty() {
            return None;
        }
        while exprs.len() > 1 {
            let rhs = exprs.pop()?;
            let lhs = exprs.pop()?;
            let merged = self.bin_and(lhs, rhs);
            exprs.push(merged);
        }
        exprs.pop()
    }

    fn match_expr(&mut self, scrutinee: ExprNode, arms: Vec<MatchArm>) -> ExprNode {
        self.node(Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
        })
    }

    fn deref_expr(&mut self, base: ExprNode) -> ExprNode {
        self.node(Expr::Unary {
            op: UnaryOp::Deref,
            operand: Box::new(base),
        })
    }

    fn self_expr(&mut self) -> ExprNode {
        let id = self.alloc_id();
        self.node(Expr::Ident(Ident {
            symbol: impl_receiver_symbol(),
            span: self.span,
            id,
        }))
    }

    fn other_expr(&mut self) -> ExprNode {
        let other = self.ident("other");
        self.ident_expr(&other)
    }

    fn ident_expr(&mut self, id: &Ident) -> ExprNode {
        self.node(Expr::Ident(*id))
    }

    fn field_access(&mut self, base: ExprNode, field: &Ident) -> ExprNode {
        let field_id = self.alloc_id();
        self.node(Expr::Postfix {
            base: Box::new(base),
            ops: vec![PostfixOp::Field(Ident {
                symbol: field.symbol,
                span: field.span,
                id: field_id,
            })],
        })
    }

    fn bin_eq(&mut self, left: ExprNode, right: ExprNode) -> ExprNode {
        self.node(Expr::Binary {
            op: BinOp::Eq,
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    fn bin_and(&mut self, left: ExprNode, right: ExprNode) -> ExprNode {
        self.node(Expr::Binary {
            op: BinOp::And,
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    fn bool_lit(&mut self, value: bool) -> ExprNode {
        self.node(Expr::Literal(Literal::Bool(value)))
    }

    fn u8_array_literal(&mut self, bytes: &[u8; 32]) -> ExprNode {
        let items: Vec<ExprNode> = bytes
            .iter()
            .map(|&b| {
                self.node(Expr::Literal(Literal::Int(IntLit {
                    value: i128::from(b),
                    suffix: IntegerSuffix::None,
                })))
            })
            .collect();
        self.node(Expr::Array(items))
    }
}

fn debug_name_bytes(name: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, b) in name.bytes().enumerate().take(32) {
        out[i] = b;
    }
    out
}

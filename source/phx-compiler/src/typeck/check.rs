//! Type-checking driver and AST walk.

// AST enums are `#[non_exhaustive]`; wildcard arms reserve future variants.
#![allow(unreachable_patterns)]

use std::collections::HashMap;

use phx_diagnostics::{Span, TypeCheckBag, TypeCheckError};
use phx_syntax::Symbol;
use phx_syntax::ast::decl::{Function, Param, StructBody, TopLevelDecl, TopLevelItem, TraitItem};
use phx_syntax::ast::expr::{Expr, PostfixOp, StructFieldInit};
use phx_syntax::ast::ident::{Ident, Path, PathSegment, TypeName};
use phx_syntax::ast::lit::Literal;
use phx_syntax::ast::pat::Pattern;
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};
use phx_syntax::ast::types::Type;
use phx_syntax::ast::{BlockNode, ExprNode, Node};
use phx_syntax::token::Keyword;

use super::bindings::{BindingKind, FunctionLayout, FunctionLayoutBuilder};
use super::builtins::{
    bool_type, float_literal_type, int_literal_type, is_copyable, u8_type, unit,
};
use super::display::format_type;
use super::lower_ty::{
    TypeDefMap, build_type_def_map, is_post_mvp_std_type, lower_type, push_generics,
};
use super::ops::{check_binary, check_cast, check_unary};
use super::ownership::OwnershipTracker;
use super::types::{ExprId, Ty, TypeId, TypeInterner};
use super::unify::unify_branch;
use crate::resolver::{DefId, DefKind, ResolutionKey, ResolvedProgram};

/// Collected struct field types.
#[derive(Debug, Clone)]
pub struct StructFields {
    /// Field name → type.
    pub fields: HashMap<Symbol, TypeId>,
}

/// Type checker state for one compilation unit.
pub struct TypeChecker<'a> {
    resolved: &'a ResolvedProgram,
    types: TypeInterner,
    bag: TypeCheckBag,
    expr_types: HashMap<ExprId, TypeId>,
    next_expr: u32,
    type_defs: TypeDefMap,
    value_types: HashMap<DefId, TypeId>,
    struct_fields: HashMap<DefId, StructFields>,
    fn_ret: Option<TypeId>,
    ownership: OwnershipTracker,
    unit: TypeId,
    bool_ty: TypeId,
    functions: Vec<FunctionLayout>,
    layout: Option<FunctionLayoutBuilder>,
    ctor_expected: Option<TypeId>,
    /// Nesting depth of `while` / `loop` bodies being checked.
    loop_depth: u32,
}

impl<'a> TypeChecker<'a> {
    fn new(resolved: &'a ResolvedProgram) -> Self {
        let mut types = TypeInterner::new();
        let unit = unit(&mut types);
        let bool_ty = bool_type(&mut types);
        Self {
            resolved,
            types,
            bag: TypeCheckBag::new(),
            expr_types: HashMap::new(),
            next_expr: 0,
            type_defs: build_type_def_map(&resolved.defs),
            value_types: HashMap::new(),
            struct_fields: HashMap::new(),
            fn_ret: None,
            ownership: OwnershipTracker::new(),
            unit,
            bool_ty,
            functions: Vec::new(),
            layout: None,
            ctor_expected: None,
            loop_depth: 0,
        }
    }

    fn error_loop_control_outside_loop(&mut self, keyword: &'static str, span: Span) {
        self.bag
            .push(TypeCheckError::LoopControlOutsideLoop { keyword, span });
    }

    fn with_loop_body<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.loop_depth = self.loop_depth.saturating_add(1);
        f(self);
        self.loop_depth = self.loop_depth.saturating_sub(1);
    }

    fn fn_def_for(&self, f: &Function) -> Option<DefId> {
        self.find_def(f.name.symbol, DefKind::Fn)
    }

    fn define_local(&mut self, symbol: phx_syntax::Symbol, ty: TypeId, kind: BindingKind) {
        self.ownership.define(symbol, ty);
        if let Some(layout) = &mut self.layout {
            let _ = layout.alloc(symbol, ty, kind);
        }
    }

    fn alloc_expr_id(&mut self) -> ExprId {
        let id = ExprId::from_raw(self.next_expr);
        self.next_expr += 1;
        id
    }

    fn format_ty(&self, id: TypeId) -> String {
        format_type(
            &self.types,
            &self.resolved.interner,
            &self.resolved.defs,
            id,
        )
    }

    fn error_mismatch(&mut self, expected: TypeId, found: TypeId, span: Span) {
        self.bag.push(TypeCheckError::Mismatch {
            expected: self.format_ty(expected),
            found: self.format_ty(found),
            span,
        });
    }

    fn lower_ast_type_with_defs(&mut self, ty: &Node<Type>, type_defs: &TypeDefMap) -> TypeId {
        if is_post_mvp_std_type(&ty.inner) {
            self.bag.push(TypeCheckError::UnsupportedFeature {
                feature: "std `Option` / `Result` types",
                span: ty.span,
            });
            return self.unit;
        }
        lower_type(&mut self.types, type_defs, &ty.inner)
    }

    fn lower_ast_type(&mut self, ty: &Node<Type>) -> TypeId {
        let type_defs = self.type_defs.clone();
        self.lower_ast_type_with_defs(ty, &type_defs)
    }

    const fn std_ctor_feature(variant: Keyword) -> &'static str {
        match variant {
            Keyword::Some | Keyword::None => "std `Option` constructors",
            Keyword::Ok | Keyword::Err => "std `Result` constructors",
            _ => "enum constructor",
        }
    }

    fn find_def(&self, name: Symbol, kind: DefKind) -> Option<DefId> {
        self.resolved
            .defs
            .iter()
            .enumerate()
            .find(|(_, d)| d.name == name && d.kind == kind)
            .map(|(i, _)| DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)))
    }

    fn lookup_resolution(&self, span: Span, symbol: Symbol) -> Option<DefId> {
        self.resolved
            .resolutions
            .get(&ResolutionKey {
                start: span.start,
                end: span.end,
                symbol,
            })
            .copied()
    }

    fn check_program(&mut self) {
        self.collect_decls();
        for item in &self.resolved.program.items {
            self.check_top_level(&item.inner);
        }
    }

    fn collect_decls(&mut self) {
        for item in &self.resolved.program.items {
            self.collect_top_level_decl(&item.inner.decl);
        }
    }

    fn collect_top_level_decl(&mut self, decl: &TopLevelDecl) {
        match decl {
            TopLevelDecl::Struct {
                name,
                generics,
                body,
            } => {
                let mut td = self.type_defs.clone();
                push_generics(&mut td, &self.resolved.defs, generics.as_deref());
                if let Some(def) = self.find_def(name.symbol, DefKind::Struct) {
                    let mut fields = HashMap::new();
                    if let StructBody::Fields(fs) = body {
                        for f in fs {
                            let ty = self.lower_ast_type_with_defs(&f.ty, &td);
                            fields.insert(f.name.symbol, ty);
                        }
                    }
                    self.struct_fields.insert(def, StructFields { fields });
                    let struct_ty = self.types.intern(&Ty::Named { def, args: vec![] });
                    self.value_types.insert(def, struct_ty);
                }
            }
            TopLevelDecl::Enum { name, .. } => {
                if let Some(def) = self.find_def(name.symbol, DefKind::Enum) {
                    let ty = self.types.intern(&Ty::Named { def, args: vec![] });
                    self.value_types.insert(def, ty);
                }
            }
            TopLevelDecl::TypeAlias { name, generics, ty } => {
                let mut td = self.type_defs.clone();
                push_generics(&mut td, &self.resolved.defs, generics.as_deref());
                if let Some(def) = self.find_def(name.symbol, DefKind::TypeAlias) {
                    let lowered = self.lower_ast_type_with_defs(ty, &td);
                    self.value_types.insert(def, lowered);
                }
            }
            TopLevelDecl::Function(f) => {
                self.collect_fn_sig(f);
            }
            TopLevelDecl::Impl {
                type_name, members, ..
            } => {
                for m in members {
                    self.collect_fn_sig(m);
                }
                let _ = type_name;
            }
            TopLevelDecl::Trait { items, .. } => {
                for item in items {
                    if let TraitItem::Method(sig) = item {
                        self.collect_fn_sig_only(sig);
                    }
                }
            }
            TopLevelDecl::Const { name, ty, .. } => {
                if let (Some(def), Some(t)) =
                    (self.find_def(name.symbol, DefKind::Const), ty.as_ref())
                {
                    let tid = self.lower_ast_type(t);
                    self.value_types.insert(def, tid);
                }
            }
            TopLevelDecl::Var { name, ty, .. } => {
                if let Some(def) = self.find_def(name.symbol, DefKind::Var) {
                    let tid = self.lower_ast_type(ty);
                    self.value_types.insert(def, tid);
                }
            }
            _ => {}
        }
    }

    fn collect_fn_sig(&mut self, f: &Function) {
        let ret = f
            .ret
            .as_ref()
            .map(|r| self.lower_ast_type(r))
            .unwrap_or(self.unit);
        let params: Vec<_> = f
            .params
            .iter()
            .filter_map(|p| match p {
                Param::Named { ty, .. } => Some(self.lower_ast_type(ty)),
                Param::Receiver { ty, .. } => ty.as_ref().map(|t| self.lower_ast_type(t)),
                _ => None,
            })
            .collect();
        let fn_ty = self.types.intern(&Ty::Fn { params, ret });
        if let Some(def) = self.find_def(f.name.symbol, DefKind::Fn) {
            self.value_types.insert(def, fn_ty);
        }
    }

    fn collect_fn_sig_only(&mut self, sig: &phx_syntax::ast::decl::FunctionSig) {
        let ret = sig
            .ret
            .as_ref()
            .map(|r| self.lower_ast_type(r))
            .unwrap_or(self.unit);
        let params: Vec<_> = sig
            .params
            .iter()
            .filter_map(|p| match p {
                Param::Named { ty, .. } => Some(self.lower_ast_type(ty)),
                Param::Receiver { ty, .. } => ty.as_ref().map(|t| self.lower_ast_type(t)),
                _ => None,
            })
            .collect();
        let fn_ty = self.types.intern(&Ty::Fn { params, ret });
        if let Some(def) = self.find_def(sig.name.symbol, DefKind::Fn) {
            self.value_types.insert(def, fn_ty);
        }
    }

    fn check_top_level(&mut self, item: &TopLevelItem) {
        match &item.decl {
            TopLevelDecl::Function(f) => self.check_function(f),
            TopLevelDecl::Const { name, ty, init } => {
                let got = self.check_expr_node(init);
                if let Some(t) = ty {
                    let expected = self.lower_ast_type(t);
                    if got != expected {
                        self.error_mismatch(expected, got, init.span);
                    }
                }
                self.ownership.define(name.symbol, got);
            }
            TopLevelDecl::Var { name, ty, init } => {
                let expected = self.lower_ast_type(ty);
                let got = self.check_expr_node(init);
                if got != expected {
                    self.error_mismatch(expected, got, init.span);
                }
                self.move_if_non_copyable(init, got);
                self.ownership.define(name.symbol, expected);
            }
            TopLevelDecl::Impl { members, .. } => {
                for m in members {
                    self.check_function(m);
                }
            }
            _ => {}
        }
    }

    fn check_function(&mut self, f: &Function) {
        let ret = f
            .ret
            .as_ref()
            .map(|r| self.lower_ast_type(r))
            .unwrap_or(self.unit);
        let def = self.fn_def_for(f).unwrap_or(DefId::from_raw(0));
        self.fn_ret = Some(ret);
        self.ownership = OwnershipTracker::new();
        self.layout = Some(FunctionLayoutBuilder::new(def, ret));
        for p in &f.params {
            if let Param::Named { name, ty, .. } = p {
                let pty = self.lower_ast_type(ty);
                self.define_local(name.symbol, pty, BindingKind::Param);
            }
        }
        let body_ty = self.check_block_value(&f.body.inner);
        if body_ty != ret {
            self.error_mismatch(ret, body_ty, f.body.span);
        }
        if let Some(builder) = self.layout.take() {
            self.functions.push(builder.finish());
        }
        self.fn_ret = None;
    }

    fn check_block(&mut self, block: &Block) {
        let _ = self.check_block_value(block);
    }

    /// Type-checks `block` and returns the type of its last value-producing item.
    fn check_block_value(&mut self, block: &Block) -> TypeId {
        let mut last = self.unit;
        for item in &block.items {
            last = match item {
                BlockItem::Stmt(stmt) => self.check_block_stmt_value(stmt),
                BlockItem::Expr(expr) => self.check_expr_node(expr),
                _ => self.unit,
            };
        }
        last
    }

    fn check_block_stmt_value(&mut self, stmt: &Stmt) -> TypeId {
        match stmt {
            Stmt::Expr(expr) => {
                if let Expr::Assign { target, value, .. } = &expr.inner {
                    let _ = self.check_assign_expr(target, value, expr.span);
                    return self.unit;
                }
                self.check_expr_node(expr)
            }
            Stmt::Return(expr) => self.check_return(expr.as_ref()),
            other => {
                self.check_stmt(other);
                self.unit
            }
        }
    }

    fn check_return(&mut self, expr: Option<&ExprNode>) -> TypeId {
        if let Some(e) = expr {
            let got = self.check_expr_node(e);
            if let Some(ret) = self.fn_ret {
                if got != ret {
                    self.error_mismatch(ret, got, e.span);
                }
            }
            got
        } else {
            if let Some(ret) = self.fn_ret {
                if ret != self.unit {
                    self.error_mismatch(self.unit, ret, Span::new(0, 0));
                }
            }
            self.unit
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Const { name, ty, init } => {
                let expected = ty.as_ref().map(|t| self.lower_ast_type(t));
                self.ctor_expected = expected;
                let got = self.check_expr_node(init);
                self.ctor_expected = None;
                if let Some(expected) = expected {
                    if got != expected {
                        self.error_mismatch(expected, got, init.span);
                    }
                }
                self.define_local(name.symbol, got, BindingKind::Const);
            }
            Stmt::Var { name, ty, init } => {
                let expected = self.lower_ast_type(ty);
                let got = self.check_expr_node(init);
                if got != expected {
                    self.error_mismatch(expected, got, init.span);
                }
                self.move_if_non_copyable(init, got);
                self.define_local(name.symbol, expected, BindingKind::Var);
            }
            Stmt::Assign { expr } => {
                if let Expr::Assign { target, value, .. } = &expr.inner {
                    let _ = self.check_assign_expr(target, value, expr.span);
                }
            }
            Stmt::Expr(expr) => {
                if let Expr::Assign { target, value, .. } = &expr.inner {
                    let _ = self.check_assign_expr(target, value, expr.span);
                } else {
                    let _ = self.check_expr_node(expr);
                }
            }
            Stmt::Return(expr) => {
                let _ = self.check_return(expr.as_ref());
            }
            Stmt::Break(expr) => {
                if self.loop_depth == 0 {
                    self.error_loop_control_outside_loop("break", loop_control_stmt_span(stmt));
                } else if let Some(e) = expr {
                    let _ = self.check_expr_node(e);
                }
            }
            Stmt::Continue => {
                if self.loop_depth == 0 {
                    self.error_loop_control_outside_loop("continue", loop_control_stmt_span(stmt));
                }
            }
            Stmt::While { cond, body } => {
                let c = self.check_expr_node(cond);
                if c != self.bool_ty {
                    self.error_mismatch(self.bool_ty, c, cond.span);
                }
                self.with_loop_body(|this| this.check_block(&body.inner));
            }
            Stmt::Loop(body) => self.with_loop_body(|this| this.check_block(&body.inner)),
            Stmt::Given {
                pattern,
                scrutinee,
                body,
            } => {
                let s = self.check_expr_node(scrutinee);
                self.check_pattern(&pattern.inner, s);
                self.check_block(&body.inner);
            }
            Stmt::Unsafe(body) => self.check_block(&body.inner),
            _ => {}
        }
    }

    fn check_assign_expr(&mut self, target: &ExprNode, value: &ExprNode, span: Span) -> TypeId {
        let lhs = self.check_assign_target(target);
        let rhs = self.check_expr_node(value);
        if lhs != rhs {
            self.error_mismatch(lhs, rhs, span);
        }
        if let Expr::Ident(ident) = &value.inner {
            if !is_copyable(&self.types, rhs) {
                self.ownership.move_binding(ident.symbol, value.span);
            }
        }
        rhs
    }

    fn check_assign_target(&mut self, target: &ExprNode) -> TypeId {
        match &target.inner {
            Expr::Ident(ident) => {
                if let Some(move_span) = self.ownership.moved_at(ident.symbol) {
                    self.bag
                        .push(TypeCheckError::MovedAssignTarget { span: target.span });
                    let _ = move_span;
                }
                self.check_ident(ident, target.span)
            }
            Expr::Postfix { base, ops } if ops.len() == 1 => {
                let base_ty = self.check_expr_node(base);
                if let PostfixOp::Field(field) = &ops[0] {
                    self.check_field(base_ty, field, target.span)
                } else {
                    self.bag.push(TypeCheckError::InvalidOperator {
                        op: "assign target",
                        span: target.span,
                    });
                    self.unit
                }
            }
            _ => {
                self.bag.push(TypeCheckError::InvalidOperator {
                    op: "assign target",
                    span: target.span,
                });
                self.unit
            }
        }
    }

    fn move_if_non_copyable(&mut self, init: &ExprNode, ty: TypeId) {
        if is_copyable(&self.types, ty) {
            return;
        }
        if let Expr::Ident(ident) = &init.inner {
            self.ownership.move_binding(ident.symbol, init.span);
        }
    }

    fn check_expr_node(&mut self, expr: &ExprNode) -> TypeId {
        let id = self.alloc_expr_id();
        let ty = self.check_expr(&expr.inner, expr.span);
        self.expr_types.insert(id, ty);
        ty
    }

    fn check_expr(&mut self, expr: &Expr, span: Span) -> TypeId {
        match expr {
            Expr::Literal(lit) => self.check_literal(lit),
            Expr::Ident(ident) => self.check_ident(ident, span),
            Expr::Path(path) => self.check_path(path, span),
            Expr::Tuple(items) => {
                let ts: Vec<_> = items.iter().map(|i| self.check_expr_node(i)).collect();
                self.types.intern(&Ty::Tuple(ts))
            }
            Expr::Array(items) => {
                let mut elem = self.unit;
                for item in items {
                    elem = self.check_expr_node(item);
                }
                let len = u32::try_from(items.len()).unwrap_or(0);
                self.types.intern(&Ty::Array { elem, len })
            }
            Expr::Unary { op, operand } => {
                let o = self.check_expr_node(operand);
                check_unary(&mut self.types, *op, o).unwrap_or_else(|| {
                    self.bag
                        .push(TypeCheckError::InvalidOperator { op: "unary", span });
                    self.unit
                })
            }
            Expr::Binary { op, left, right } => {
                let l = self.check_expr_node(left);
                let r = self.check_expr_node(right);
                check_binary(&mut self.types, *op, l, r)
                    .map(|r| r.result)
                    .unwrap_or_else(|| {
                        self.bag
                            .push(TypeCheckError::InvalidOperator { op: "binary", span });
                        self.unit
                    })
            }
            Expr::Assign { target, value, .. } => self.check_assign_expr(target, value, span),
            Expr::EnumCtor { variant, inner } => {
                if let Some(inner_expr) = inner {
                    let _ = self.check_expr_node(inner_expr);
                }
                self.bag.push(TypeCheckError::UnsupportedFeature {
                    feature: Self::std_ctor_feature(*variant),
                    span,
                });
                self.unit
            }
            Expr::Cast { expr, ty } => {
                let from = self.check_expr_node(expr);
                let td = self.type_defs.clone();
                let to = self.lower_ast_type_with_defs(ty, &td);
                if !check_cast(&self.types, from, to) {
                    self.bag.push(TypeCheckError::InvalidCast {
                        from: self.format_ty(from),
                        to: self.format_ty(to),
                        span,
                    });
                }
                to
            }
            Expr::Postfix { base, ops } => self.check_postfix(base, ops, span),
            Expr::If {
                cond,
                then_block,
                else_ifs,
                else_block,
            } => self.check_if(cond, then_block, else_ifs, else_block, span),
            Expr::Match { scrutinee, arms } => self.check_match(scrutinee, arms, span),
            Expr::Block(block) => self.check_block_expr(block),
            Expr::StructLit { name, fields, .. } => self.check_struct_lit(name, fields, span),
            Expr::Unsafe(block) => self.check_block_expr(block),
            _ => self.unit,
        }
    }

    fn check_block_expr(&mut self, block: &BlockNode) -> TypeId {
        self.check_block_value(&block.inner)
    }

    fn check_literal(&mut self, lit: &Literal) -> TypeId {
        match lit {
            Literal::Int(i) => int_literal_type(
                &mut self.types,
                matches!(i.suffix, phx_syntax::token::IntegerSuffix::Unsigned),
            ),
            Literal::Float(f) => float_literal_type(&mut self.types, f.suffix),
            Literal::Bool(_) => self.bool_ty,
            Literal::ByteChar(_) => u8_type(&mut self.types),
            Literal::ByteString(b) => {
                let len = u32::try_from(b.len()).unwrap_or(0);
                let u8 = u8_type(&mut self.types);
                self.types.intern(&Ty::Array { elem: u8, len })
            }
            _ => self.unit,
        }
    }

    fn check_ident(&mut self, ident: &Ident, span: Span) -> TypeId {
        if let Some(move_span) = self.ownership.moved_at(ident.symbol) {
            self.bag.push(TypeCheckError::UseAfterMove {
                symbol_index: ident.symbol.index(),
                move_span,
                span,
            });
        }
        if let Some(ty) = self.ownership.binding_type(ident.symbol).or_else(|| {
            self.lookup_resolution(span, ident.symbol)
                .and_then(|def| self.value_types.get(&def).copied())
        }) {
            if !is_copyable(&self.types, ty) {
                self.ownership.move_binding(ident.symbol, span);
            }
            return ty;
        }
        if let Some(def) = self.lookup_resolution(span, ident.symbol) {
            if let Some(fields) = self.struct_fields.get(&def) {
                let _ = fields;
            }
        }
        self.bag.push(TypeCheckError::UnresolvedValue {
            symbol_index: ident.symbol.index(),
            span,
        });
        self.unit
    }

    fn check_path(&mut self, path: &Path, span: Span) -> TypeId {
        if path.segments.len() == 1 {
            match &path.segments[0] {
                PathSegment::Ident(ident) => return self.check_ident(ident, span),
                PathSegment::Type(name) => {
                    if let Some(def) = self.type_defs.get(&name.symbol).copied() {
                        return self.value_types.get(&def).copied().unwrap_or_else(|| {
                            self.types.intern(&Ty::Named { def, args: vec![] })
                        });
                    }
                }
            }
        }
        self.unit
    }

    fn check_postfix(&mut self, base: &ExprNode, ops: &[PostfixOp], span: Span) -> TypeId {
        let mut ty = self.check_expr_node(base);
        for op in ops {
            ty = match op {
                PostfixOp::Field(name) => self.check_field(ty, name, span),
                PostfixOp::Method { name, args, .. } => {
                    self.check_method_call(ty, name, args, span)
                }
                PostfixOp::Call(args) => self.check_call(ty, args, span),
                PostfixOp::Index(idx) => {
                    let _ = self.check_expr_node(idx);
                    self.check_index(ty, span)
                }
                PostfixOp::Try => {
                    self.bag.push(TypeCheckError::UnsupportedFeature {
                        feature: "`?` operator (requires std `Option` / `Result`)",
                        span,
                    });
                    self.unit
                }
                _ => self.unit,
            };
        }
        ty
    }

    fn check_field(&mut self, base: TypeId, field: &Ident, span: Span) -> TypeId {
        if let Ty::Named { def, .. } = self.types.get(base) {
            let def = *def;
            if let Some(sf) = self.struct_fields.get(&def) {
                if let Some(&fty) = sf.fields.get(&field.symbol) {
                    return fty;
                }
            }
        }
        self.bag.push(TypeCheckError::UnresolvedMethod {
            receiver: self.format_ty(base),
            method_index: field.symbol.index(),
            span,
        });
        self.unit
    }

    fn check_call(&mut self, callee: TypeId, args: &[ExprNode], span: Span) -> TypeId {
        let Ty::Fn { params, ret } = self.types.get(callee).clone() else {
            self.bag.push(TypeCheckError::NotCallable {
                found: self.format_ty(callee),
                span,
            });
            return self.unit;
        };
        {
            if params.len() != args.len() {
                self.bag.push(TypeCheckError::ArityMismatch {
                    expected: params.len(),
                    found: args.len(),
                    span,
                });
            }
            for (p, arg) in params.iter().zip(args.iter()) {
                let got = self.check_expr_node(arg);
                if got != *p {
                    self.error_mismatch(*p, got, arg.span);
                }
            }
            ret
        }
    }

    fn check_index(&mut self, base: TypeId, span: Span) -> TypeId {
        match self.types.get(base) {
            Ty::Array { elem, .. } | Ty::Slice(elem) => *elem,
            _ => {
                self.bag
                    .push(TypeCheckError::InvalidOperator { op: "index", span });
                self.unit
            }
        }
    }

    fn check_method_call(
        &mut self,
        receiver: TypeId,
        name: &Ident,
        args: &[ExprNode],
        span: Span,
    ) -> TypeId {
        let _ = receiver;
        if let Some(def) = self.lookup_resolution(span, name.symbol) {
            if let Some(&fn_ty) = self.value_types.get(&def) {
                return self.check_call(fn_ty, args, span);
            }
        }
        self.bag.push(TypeCheckError::UnresolvedMethod {
            receiver: self.format_ty(receiver),
            method_index: name.symbol.index(),
            span,
        });
        self.unit
    }

    fn check_if(
        &mut self,
        cond: &ExprNode,
        then_block: &BlockNode,
        else_ifs: &[(ExprNode, BlockNode)],
        else_block: &Option<BlockNode>,
        span: Span,
    ) -> TypeId {
        let c = self.check_expr_node(cond);
        if c != self.bool_ty {
            self.error_mismatch(self.bool_ty, c, cond.span);
        }
        let mut then_ty = self.check_block_expr(then_block);
        for (ec, eb) in else_ifs {
            let e = self.check_expr_node(ec);
            if e != self.bool_ty {
                self.error_mismatch(self.bool_ty, e, ec.span);
            }
            let arm_ty = self.check_block_expr(eb);
            then_ty = unify_branch(&self.types, then_ty, arm_ty).unwrap_or_else(|| {
                self.bag.push(TypeCheckError::NonUnifyingBranches { span });
                self.unit
            });
        }
        if let Some(else_b) = else_block {
            let arm_ty = self.check_block_expr(else_b);
            then_ty = unify_branch(&self.types, then_ty, arm_ty).unwrap_or_else(|| {
                self.bag.push(TypeCheckError::NonUnifyingBranches { span });
                self.unit
            });
        }
        then_ty
    }

    fn check_match(
        &mut self,
        scrutinee: &ExprNode,
        arms: &[phx_syntax::ast::pat::MatchArm],
        span: Span,
    ) -> TypeId {
        let s = self.check_expr_node(scrutinee);
        let mut acc: Option<TypeId> = None;
        for arm in arms {
            self.check_pattern(&arm.pattern.inner, s);
            if let Some(g) = &arm.guard {
                let gt = self.check_expr_node(g);
                if gt != self.bool_ty {
                    self.error_mismatch(self.bool_ty, gt, g.span);
                }
            }
            let body_ty = self.check_expr_node(&arm.body);
            acc = Some(match acc {
                None => body_ty,
                Some(prev) => unify_branch(&self.types, prev, body_ty).unwrap_or_else(|| {
                    self.bag.push(TypeCheckError::NonUnifyingBranches { span });
                    self.unit
                }),
            });
        }
        acc.unwrap_or(self.unit)
    }

    fn check_pattern(&mut self, pat: &Pattern, scrutinee: TypeId) {
        match pat {
            Pattern::Wildcard | Pattern::Literal(_) => {}
            Pattern::Ident(ident) => {
                self.define_local(ident.symbol, scrutinee, BindingKind::Var);
            }
            Pattern::Struct { .. } | Pattern::Tuple { .. } | Pattern::EnumCtor { .. } => {}
            _ => {}
        }
    }

    fn check_struct_lit(
        &mut self,
        name: &TypeName,
        fields: &[StructFieldInit],
        span: Span,
    ) -> TypeId {
        if let Some(&def) = self.type_defs.get(&name.symbol) {
            let ty = self.types.intern(&Ty::Named { def, args: vec![] });
            let field_types: Vec<_> = fields
                .iter()
                .filter_map(|field| {
                    let StructFieldInit::Field { name: fname, value } = field else {
                        return None;
                    };
                    let expected = self
                        .struct_fields
                        .get(&def)
                        .and_then(|sf| sf.fields.get(&fname.symbol).copied())?;
                    Some((expected, value))
                })
                .collect();
            for (expected, value) in field_types {
                let got = self.check_expr_node(value);
                if got != expected {
                    self.error_mismatch(expected, got, value.span);
                }
            }
            return ty;
        }
        self.bag.push(TypeCheckError::UnknownType {
            symbol_index: name.symbol.index(),
            span,
        });
        self.unit
    }

    fn finish(
        self,
    ) -> (
        TypeInterner,
        HashMap<ExprId, TypeId>,
        TypeCheckBag,
        Vec<FunctionLayout>,
    ) {
        (self.types, self.expr_types, self.bag, self.functions)
    }
}

/// Best-effort span for `break` / `continue` (statement nodes are not spanned in blocks).
fn loop_control_stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Break(Some(e)) => e.span,
        Stmt::Break(None) | Stmt::Continue => Span::new(0, 0),
        _ => Span::new(0, 0),
    }
}

/// Runs type checking on `resolved`.
///
/// # Errors
///
/// Returns [`TypeCheckBag`] when one or more type errors were collected.
pub fn type_check(resolved: &ResolvedProgram) -> Result<super::TypedProgram, TypeCheckBag> {
    let mut checker = TypeChecker::new(resolved);
    checker.check_program();
    let (types, expr_types, bag, functions) = checker.finish();
    if bag.has_errors() {
        return Err(bag);
    }
    Ok(super::TypedProgram {
        resolved: resolved.clone(),
        types,
        expr_types,
        functions,
        entry: resolved.main_fn,
    })
}

//! AST traversal for name resolution.
//!
//! Pass order: reject `#import` → collect top-level defs → resolve bodies/types/exprs → check
//! `main`. Uses separate value and type namespaces per [`super::scopes::ScopeStack`].

// AST enums are `#[non_exhaustive]`; wildcard arms reserve future variants.
#![allow(
    unreachable_patterns,
    clippy::match_same_arms,
    clippy::ref_option,
    clippy::collapsible_match,
    clippy::trivially_copy_pass_by_ref,
    clippy::collapsible_if
)]

use phx_diagnostics::{InvalidMainReason, ResolveError, Span};
use phx_syntax::ast::decl::{
    Function, FunctionSig, Param, StructBody, TopLevelDecl, TraitItem, Variant,
};
use phx_syntax::ast::expr::{Expr, PostfixOp, StructFieldInit};
use phx_syntax::ast::ident::{Ident, Path, PathSegment, TypeName};
use phx_syntax::ast::pat::{MatchArm, Pattern};
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};
use phx_syntax::ast::types::{GenericParam, Type};
use phx_syntax::ast::{BlockNode, ExprNode, Node, PatternNode};
use phx_syntax::{Symbol, impl_receiver_symbol};

use super::Resolver;
use super::def_id::DefKind;

impl Resolver<'_> {
    /// Resolves the whole program in `self.source`.
    /// Runs the full resolve pass on [`Resolver::source`].
    pub(super) fn resolve_program(&mut self) {
        for import in &self.source.program.imports {
            self.bag
                .push(ResolveError::ImportNotSupported { span: import.span });
        }

        self.scopes.push();
        self.collect_top_level_defs();
        self.resolve_top_level_items();
        self.check_main();
        self.scopes.pop();
    }

    fn collect_top_level_defs(&mut self) {
        for item in &self.source.program.items {
            self.collect_top_level_decl(&item.inner.decl, item.span);
        }
    }

    fn collect_top_level_decl(&mut self, decl: &TopLevelDecl, span: Span) {
        match decl {
            TopLevelDecl::Struct { name, .. } => {
                self.define_type(name.symbol, span, DefKind::Struct);
            }
            TopLevelDecl::Enum { name, variants, .. } => {
                self.define_type(name.symbol, span, DefKind::Enum);
                for v in variants {
                    self.define_value(v.name.symbol, name_span_type(&v.name), DefKind::EnumVariant);
                }
            }
            TopLevelDecl::TypeAlias { name, .. } => {
                self.define_type(name.symbol, span, DefKind::TypeAlias);
            }
            TopLevelDecl::Trait { name, .. } => {
                self.define_type(name.symbol, span, DefKind::Trait);
            }
            TopLevelDecl::Impl { members, .. } => {
                for member in members {
                    let span = name_span_ident(&member.name);
                    self.define_value(member.name.symbol, span, DefKind::Fn);
                }
            }
            TopLevelDecl::Function(f) => {
                let id = self.define_value(f.name.symbol, span, DefKind::Fn);
                if self.is_main_name(f.name.symbol) {
                    self.main_fn = Some(id);
                }
            }
            TopLevelDecl::Const { name, .. } => {
                self.define_value(name.symbol, span, DefKind::Const);
            }
            TopLevelDecl::Var { name, .. } => {
                self.define_value(name.symbol, span, DefKind::Var);
            }
            _ => {}
        }
    }

    fn resolve_top_level_items(&mut self) {
        for item in &self.source.program.items {
            self.resolve_top_level_decl(&item.inner.decl);
        }
    }

    fn resolve_top_level_decl(&mut self, decl: &TopLevelDecl) {
        match decl {
            TopLevelDecl::Struct { generics, body, .. } => {
                self.scopes.push();
                self.resolve_generics(generics);
                self.resolve_struct_body(body);
                self.scopes.pop();
            }
            TopLevelDecl::Enum {
                generics, variants, ..
            } => {
                self.scopes.push();
                self.resolve_generics(generics);
                for v in variants {
                    self.resolve_enum_variant_kind(&v.kind);
                }
                self.scopes.pop();
            }
            TopLevelDecl::TypeAlias { generics, ty, .. } => {
                self.scopes.push();
                self.resolve_generics(generics);
                self.resolve_type_node(ty);
                self.scopes.pop();
            }
            TopLevelDecl::Trait {
                generics, items, ..
            } => {
                self.scopes.push();
                self.resolve_generics(generics);
                for item in items {
                    self.resolve_trait_item(item);
                }
                self.scopes.pop();
            }
            TopLevelDecl::Impl {
                generics, members, ..
            } => {
                self.scopes.push();
                self.resolve_generics(generics);
                for member in members {
                    self.resolve_function(member, true);
                }
                self.scopes.pop();
            }
            TopLevelDecl::Function(f) => self.resolve_function(f, false),
            TopLevelDecl::Const { ty, init, .. } => {
                if let Some(t) = ty {
                    self.resolve_type_node(t);
                }
                self.resolve_expr_node(init);
            }
            TopLevelDecl::Var { ty, init, .. } => {
                self.resolve_type_node(ty);
                self.resolve_expr_node(init);
            }
            _ => {}
        }
    }

    fn resolve_trait_item(&mut self, item: &TraitItem) {
        match item {
            TraitItem::AssociatedType(name) => {
                let span = name_span_ident(name);
                self.define_type(name.symbol, span, DefKind::TraitAssocType);
            }
            TraitItem::Method(sig) => self.resolve_function_sig(sig),
            _ => {}
        }
    }

    fn resolve_struct_body(&mut self, body: &StructBody) {
        match body {
            StructBody::Fields(fields) => {
                for field in fields {
                    self.resolve_type_node(&field.ty);
                }
            }
            StructBody::Tuple(types) => {
                for t in types {
                    self.resolve_type_node(t);
                }
            }
            StructBody::Unit => {}
            _ => {}
        }
    }

    fn resolve_enum_variant_kind(&mut self, kind: &Variant) {
        match kind {
            Variant::Unit => {}
            Variant::Struct(fields) => {
                for field in fields {
                    self.resolve_type_node(&field.ty);
                }
            }
            Variant::Tuple(types) => {
                for t in types {
                    self.resolve_type_node(t);
                }
            }
            _ => {}
        }
    }

    fn resolve_generics(&mut self, generics: &Option<Vec<GenericParam>>) {
        if let Some(params) = generics {
            for param in params {
                let span = name_span_ident(&param.name);
                self.define_type(param.name.symbol, span, DefKind::GenericParam);
                if let Some(bounds) = &param.bounds {
                    for bound in bounds {
                        self.resolve_type_name(bound, name_span_type(bound));
                    }
                }
            }
        }
    }

    fn resolve_function(&mut self, f: &Function, in_impl: bool) {
        self.scopes.push();
        self.resolve_generics(&f.generics);
        let has_receiver = f.params.iter().any(|p| matches!(p, Param::Receiver { .. }));
        if in_impl && !has_receiver {
            self.define_value(impl_receiver_symbol(), Span::new(0, 0), DefKind::Param);
        }
        self.resolve_params(&f.params);
        if let Some(ret) = &f.ret {
            self.resolve_type_node(ret);
        }
        self.resolve_block_node(&f.body);
        self.scopes.pop();
    }

    fn resolve_function_sig(&mut self, sig: &FunctionSig) {
        self.scopes.push();
        self.resolve_generics(&sig.generics);
        self.resolve_params(&sig.params);
        if let Some(ret) = &sig.ret {
            self.resolve_type_node(ret);
        }
        if let Some(body) = &sig.body {
            self.resolve_block_node(body);
        }
        self.scopes.pop();
    }

    fn resolve_params(&mut self, params: &[Param]) {
        for param in params {
            match param {
                Param::Receiver { ty, .. } => {
                    self.define_value(impl_receiver_symbol(), Span::new(0, 0), DefKind::Param);
                    if let Some(t) = ty {
                        self.resolve_type_node(t);
                    }
                }
                Param::Named { name, ty } => {
                    self.define_value(name.symbol, name_span_ident(name), DefKind::Param);
                    self.resolve_type_node(ty);
                }
                _ => {}
            }
        }
    }

    fn resolve_block_node(&mut self, block: &BlockNode) {
        self.scopes.push();
        self.resolve_block(&block.inner);
        self.scopes.pop();
    }

    fn resolve_block(&mut self, block: &Block) {
        for item in &block.items {
            self.resolve_block_item(item);
        }
    }

    fn resolve_block_item(&mut self, item: &BlockItem) {
        match item {
            BlockItem::Stmt(stmt) => self.resolve_stmt(stmt),
            BlockItem::Expr(expr) => self.resolve_expr_node(expr),
            _ => {}
        }
    }

    fn resolve_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Const { name, ty, init } => {
                self.define_value(name.symbol, name_span_ident(name), DefKind::Local);
                if let Some(t) = ty {
                    self.resolve_type_node(t);
                }
                self.resolve_expr_node(init);
            }
            Stmt::Var { name, ty, init } => {
                self.define_value(name.symbol, name_span_ident(name), DefKind::Local);
                self.resolve_type_node(ty);
                self.resolve_expr_node(init);
            }
            Stmt::Assign { expr } => self.resolve_expr_node(expr),
            Stmt::Expr(expr) => self.resolve_expr_node(expr),
            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    self.resolve_expr_node(e);
                }
            }
            Stmt::Break(expr) => {
                if let Some(e) = expr {
                    self.resolve_expr_node(e);
                }
            }
            Stmt::Continue => {}
            Stmt::While { cond, body } => {
                self.resolve_expr_node(cond);
                self.resolve_block_node(body);
            }
            Stmt::Loop(body) => self.resolve_block_node(body),
            Stmt::Given {
                pattern,
                scrutinee,
                body,
            } => {
                self.resolve_expr_node(scrutinee);
                self.scopes.push();
                self.resolve_pattern_node(pattern);
                self.resolve_block_node(body);
                self.scopes.pop();
            }
            Stmt::Unsafe(body) => self.resolve_block_node(body),
            _ => {}
        }
    }

    fn resolve_type_node(&mut self, ty: &Node<Type>) {
        self.resolve_type(&ty.inner, ty.span);
    }

    fn resolve_type(&mut self, ty: &Type, span: Span) {
        match ty {
            Type::Primitive(_) => {}
            Type::Named { name, generics } => {
                self.resolve_type_name(name, span);
                if let Some(args) = generics {
                    for arg in args {
                        self.resolve_type_node(arg);
                    }
                }
            }
            Type::Function { params, ret } => {
                for p in params {
                    self.resolve_type_node(p);
                }
                self.resolve_type_node(ret);
            }
            Type::Ref { inner, .. } | Type::Ptr { inner, .. } => {
                self.resolve_type_node(inner);
            }
            Type::Tuple(types) => {
                for t in types {
                    self.resolve_type_node(t);
                }
            }
            Type::Unit => {}
            Type::Array { elem, .. } => self.resolve_type_node(elem),
            Type::Slice(inner) => self.resolve_type_node(inner),
            _ => {}
        }
    }

    fn resolve_type_name(&mut self, name: &TypeName, span: Span) {
        let def_id = self.scopes.lookup_type(name.symbol);
        if def_id.is_some() {
            self.record_resolution(span, name.symbol, def_id);
        } else {
            self.bag.push(ResolveError::UnresolvedType {
                symbol_index: name.symbol.index(),
                span,
            });
        }
    }

    /// Resolves a `PascalCase` name in expression position (enum variant ctors before types).
    fn resolve_type_or_value_name(&mut self, name: &TypeName, span: Span) {
        if let Some(id) = self.scopes.lookup_value(name.symbol) {
            self.record_resolution(span, name.symbol, Some(id));
            return;
        }
        self.resolve_type_name(name, span);
    }

    fn resolve_expr_node(&mut self, expr: &ExprNode) {
        self.resolve_expr(&expr.inner, expr.span);
    }

    fn resolve_expr(&mut self, expr: &Expr, span: Span) {
        match expr {
            Expr::Literal(_) => {}
            Expr::Ident(ident) => self.resolve_ident(ident, span),
            Expr::Path(path) => self.resolve_path_expr(path, span),
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    self.resolve_expr_node(item);
                }
            }
            Expr::Unary { operand, .. } => self.resolve_expr_node(operand),
            Expr::Binary { left, right, .. } => {
                self.resolve_expr_node(left);
                self.resolve_expr_node(right);
            }
            Expr::Assign { target, value, .. } => {
                self.resolve_expr_node(target);
                self.resolve_expr_node(value);
            }
            Expr::Cast { expr, ty } => {
                self.resolve_expr_node(expr);
                self.resolve_type_node(ty);
            }
            Expr::Postfix { base, ops } => {
                self.resolve_expr_node(base);
                for op in ops {
                    self.resolve_postfix_op(op);
                }
            }
            Expr::If {
                cond,
                then_block,
                else_ifs,
                else_block,
            } => {
                self.resolve_expr_node(cond);
                self.resolve_block_node(then_block);
                for (c, b) in else_ifs {
                    self.resolve_expr_node(c);
                    self.resolve_block_node(b);
                }
                if let Some(b) = else_block {
                    self.resolve_block_node(b);
                }
            }
            Expr::Match { scrutinee, arms } => {
                self.resolve_expr_node(scrutinee);
                for arm in arms {
                    self.resolve_match_arm(arm);
                }
            }
            Expr::Block(block) => self.resolve_block_node(block),
            Expr::StructLit {
                name,
                generics,
                fields,
            } => {
                self.resolve_type_or_value_name(name, span);
                if let Some(args) = generics {
                    for arg in args {
                        self.resolve_type_node(arg);
                    }
                }
                for field in fields {
                    match field {
                        StructFieldInit::Field { value, .. } => {
                            self.resolve_expr_node(value);
                        }
                        StructFieldInit::Spread(expr) => self.resolve_expr_node(expr),
                        _ => {}
                    }
                }
            }
            Expr::Unsafe(block) => self.resolve_block_node(block),
            _ => {}
        }
    }

    fn resolve_postfix_op(&mut self, op: &PostfixOp) {
        match op {
            PostfixOp::Field(_) => {}
            PostfixOp::Method { args, generics, .. } => {
                if let Some(g) = generics {
                    for arg in g {
                        self.resolve_type_node(arg);
                    }
                }
                for arg in args {
                    self.resolve_expr_node(arg);
                }
            }
            PostfixOp::Call(args) => {
                for arg in args {
                    self.resolve_expr_node(arg);
                }
            }
            PostfixOp::Index(expr) => self.resolve_expr_node(expr),
            PostfixOp::Try => {}
            _ => {}
        }
    }

    fn resolve_match_arm(&mut self, arm: &MatchArm) {
        self.scopes.push();
        self.resolve_pattern_node(&arm.pattern);
        if let Some(guard) = &arm.guard {
            self.resolve_expr_node(guard);
        }
        self.resolve_expr_node(&arm.body);
        self.scopes.pop();
    }

    fn resolve_pattern_node(&mut self, pat: &PatternNode) {
        self.resolve_pattern(&pat.inner, pat.span);
    }

    fn resolve_pattern(&mut self, pat: &Pattern, span: Span) {
        match pat {
            Pattern::Wildcard | Pattern::Literal(_) => {}
            Pattern::Ident(ident) => {
                self.define_value(ident.symbol, span, DefKind::Local);
            }
            Pattern::Struct { name, fields } => {
                self.resolve_type_or_value_name(name, span);
                for field in fields {
                    if let Some(p) = &field.pattern {
                        self.resolve_pattern_node(p);
                    } else {
                        self.define_value(field.name.symbol, span, DefKind::Local);
                    }
                }
            }
            Pattern::Tuple { name, patterns } => {
                self.resolve_type_or_value_name(name, span);
                for p in patterns {
                    self.resolve_pattern_node(p);
                }
            }
            _ => {}
        }
    }

    fn resolve_ident(&mut self, ident: &Ident, span: Span) {
        let def_id = self.scopes.lookup_value(ident.symbol);
        if let Some(id) = def_id {
            self.record_resolution(span, ident.symbol, Some(id));
        } else {
            self.bag.push(ResolveError::UnresolvedIdent {
                symbol_index: ident.symbol.index(),
                span,
            });
        }
    }

    fn resolve_path_expr(&mut self, path: &Path, span: Span) {
        if path.segments.is_empty() {
            return;
        }
        match &path.segments[0] {
            PathSegment::Ident(ident) => self.resolve_ident(ident, span),
            PathSegment::Type(name) => self.resolve_type_or_value_name(name, span),
        }
        if path.segments.len() > 1 {
            for seg in &path.segments[1..] {
                match seg {
                    PathSegment::Ident(ident) => {
                        self.resolve_ident(ident, span);
                    }
                    PathSegment::Type(name) => {
                        self.resolve_type_name(name, span);
                    }
                }
            }
        }
    }

    /// Validates MVP entry `main :: () => { … }`.
    fn check_main(&mut self) {
        if self.main_fn.is_none() {
            self.bag.push(ResolveError::MissingMain);
            return;
        }

        let mut has_params = false;
        let mut bad_ret_span = None;

        for item in &self.source.program.items {
            let TopLevelDecl::Function(f) = &item.inner.decl else {
                continue;
            };
            if !self.is_main_name(f.name.symbol) {
                continue;
            }
            has_params = !f.params.is_empty();
            if let Some(ret) = &f.ret {
                if !type_is_unit(&ret.inner) {
                    bad_ret_span = Some(ret.span);
                }
            }
            break;
        }

        if has_params {
            self.bag.push(ResolveError::InvalidMainSignature {
                span: Span::new(0, 0),
                reason: InvalidMainReason::HasParameters,
            });
        }
        if let Some(span) = bad_ret_span {
            self.bag.push(ResolveError::InvalidMainSignature {
                span,
                reason: InvalidMainReason::NonUnitReturn,
            });
        }
    }

    pub(crate) fn is_main_name(&self, symbol: Symbol) -> bool {
        self.source.interner.resolve(symbol) == "main"
    }
}

fn type_is_unit(ty: &Type) -> bool {
    matches!(ty, Type::Unit)
}

fn name_span_ident(_ident: &Ident) -> Span {
    Span::new(0, 0)
}

fn name_span_type(_name: &TypeName) -> Span {
    Span::new(0, 0)
}

//! Type-checking driver and AST walk.

// AST enums are `#[non_exhaustive]`; wildcard arms reserve future variants.
#![allow(unreachable_patterns)]

use std::collections::HashMap;

use phx_diagnostics::{Span, TypeCheckBag, TypeCheckError};
use phx_syntax::ast::decl::{
    Function, Param, StructBody, TopLevelDecl, TopLevelItem, TraitItem, Variant,
};
use phx_syntax::ast::expr::{Expr, PostfixOp, StructFieldInit};
use phx_syntax::ast::ident::{Ident, Path, PathSegment, TypeName};
use phx_syntax::ast::lit::Literal;
use phx_syntax::ast::pat::Pattern;
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};
use phx_syntax::ast::types::Type;
use phx_syntax::ast::{BlockNode, ExprNode, Node};
use phx_syntax::{Symbol, impl_receiver_symbol};

use super::bindings::{BindingKind, FunctionLayout, FunctionLayoutBuilder};
use super::builtins::{
    bool_type, float_literal_type, int_literal_type, is_copyable, u8_type, unit,
};
use super::display::format_type;
use super::layout::{
    EnumLayout, ProgramLayout, StructLayout, VariantKind, VariantLayout, VariantMeta,
};
use super::lower_ty::{TypeDefMap, build_type_def_map, lower_type, push_generics};
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
    /// Struct/enum layouts for lowering and codegen.
    program_layout: ProgramLayout,
    next_type_id: u32,
    /// When checking inherent impl members, the receiver type (`Self`).
    impl_self_type: Option<TypeId>,
    /// Module being collected or checked.
    current_module: u32,
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
            program_layout: ProgramLayout::default(),
            next_type_id: 1,
            impl_self_type: None,
            current_module: resolved.root,
        }
    }

    fn alloc_type_id(&mut self, def: DefId) -> u32 {
        let id = self.next_type_id;
        self.next_type_id += 1;
        self.program_layout.type_ids.insert(def, id);
        id
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
        self.find_def(self.current_module, f.name.symbol, DefKind::Fn)
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
        lower_type(&mut self.types, type_defs, &ty.inner)
    }

    fn lower_ast_type(&mut self, ty: &Node<Type>) -> TypeId {
        let type_defs = self.type_defs.clone();
        self.lower_ast_type_with_defs(ty, &type_defs)
    }

    fn find_def(&self, module: u32, name: Symbol, kind: DefKind) -> Option<DefId> {
        self.resolved
            .defs
            .iter()
            .enumerate()
            .find(|(_, d)| d.module == module && d.name == name && d.kind == kind)
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
        for module in &self.resolved.modules {
            self.current_module = module.id;
            for item in &module.program.items {
                self.check_top_level(&item.inner);
            }
        }
    }

    fn collect_decls(&mut self) {
        for module in &self.resolved.modules {
            self.current_module = module.id;
            for item in &module.program.items {
                self.collect_top_level_decl(&item.inner.decl);
            }
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
                if let Some(def) = self.find_def(self.current_module, name.symbol, DefKind::Struct)
                {
                    let mut fields_map = HashMap::new();
                    let mut ordered = Vec::new();
                    if let StructBody::Fields(fs) = body {
                        for f in fs {
                            let ty = self.lower_ast_type_with_defs(&f.ty, &td);
                            fields_map.insert(f.name.symbol, ty);
                            ordered.push((f.name.symbol, ty));
                        }
                    }
                    self.struct_fields
                        .insert(def, StructFields { fields: fields_map });
                    self.program_layout
                        .structs
                        .insert(def, StructLayout { fields: ordered });
                    let _ = self.alloc_type_id(def);
                    let struct_ty = self.types.intern(&Ty::Named { def, args: vec![] });
                    self.value_types.insert(def, struct_ty);
                }
            }
            TopLevelDecl::Enum {
                name,
                generics,
                variants,
            } => {
                let mut td = self.type_defs.clone();
                push_generics(&mut td, &self.resolved.defs, generics.as_deref());
                if let Some(enum_def) =
                    self.find_def(self.current_module, name.symbol, DefKind::Enum)
                {
                    let enum_ty = self.types.intern(&Ty::Named {
                        def: enum_def,
                        args: vec![],
                    });
                    self.value_types.insert(enum_def, enum_ty);
                    let type_id = self.alloc_type_id(enum_def);
                    let _ = type_id;
                    let mut variant_layouts = Vec::new();
                    for (tag, v) in variants.iter().enumerate() {
                        let tag = u32::try_from(tag).unwrap_or(u32::MAX);
                        let variant_def =
                            self.find_def(self.current_module, v.name.symbol, DefKind::EnumVariant);
                        let (payload_types, kind) = match &v.kind {
                            Variant::Unit => (vec![], VariantKind::Unit),
                            Variant::Tuple(ts) => {
                                let pts: Vec<TypeId> = ts
                                    .iter()
                                    .map(|t| self.lower_ast_type_with_defs(t, &td))
                                    .collect();
                                (pts.clone(), VariantKind::Tuple(pts))
                            }
                            Variant::Struct(fs) => {
                                let fields: Vec<(Symbol, TypeId)> = fs
                                    .iter()
                                    .map(|f| {
                                        (f.name.symbol, self.lower_ast_type_with_defs(&f.ty, &td))
                                    })
                                    .collect();
                                let pts: Vec<TypeId> = fields.iter().map(|(_, ty)| *ty).collect();
                                (pts, VariantKind::Struct(fields))
                            }
                            _ => (vec![], VariantKind::Unit),
                        };
                        if let Some(vdef) = variant_def {
                            let params: Vec<TypeId> = payload_types.clone();
                            let ctor_ty = self.types.intern(&Ty::Fn {
                                params,
                                ret: enum_ty,
                            });
                            self.value_types.insert(vdef, ctor_ty);
                            self.program_layout.variants.insert(
                                vdef,
                                VariantMeta {
                                    enum_def,
                                    tag,
                                    payload: kind.clone(),
                                },
                            );
                            variant_layouts.push(VariantLayout {
                                def: vdef,
                                name: v.name.symbol,
                                tag,
                                kind,
                            });
                        }
                    }
                    self.program_layout.enums.insert(
                        enum_def,
                        EnumLayout {
                            enum_def,
                            variants: variant_layouts,
                        },
                    );
                }
            }
            TopLevelDecl::TypeAlias { name, generics, ty } => {
                let mut td = self.type_defs.clone();
                push_generics(&mut td, &self.resolved.defs, generics.as_deref());
                if let Some(def) =
                    self.find_def(self.current_module, name.symbol, DefKind::TypeAlias)
                {
                    let lowered = self.lower_ast_type_with_defs(ty, &td);
                    self.value_types.insert(def, lowered);
                }
            }
            TopLevelDecl::Function(f) => {
                self.collect_fn_sig(f);
            }
            TopLevelDecl::Impl {
                type_name,
                trait_,
                members,
                ..
            } => {
                if let Some(type_def) = self.type_defs.get(&type_name.symbol).copied() {
                    for m in members {
                        self.collect_fn_sig(m);
                        if let Some(fn_def) =
                            self.find_def(self.current_module, m.name.symbol, DefKind::Fn)
                        {
                            if let Some(trait_name) = trait_ {
                                if let Some(trait_def) =
                                    self.type_defs.get(&trait_name.symbol).copied()
                                {
                                    self.program_layout
                                        .trait_methods
                                        .insert((type_def, trait_def, m.name.symbol), fn_def);
                                }
                            } else {
                                self.program_layout
                                    .inherent_methods
                                    .insert((type_def, m.name.symbol), fn_def);
                            }
                        }
                    }
                } else {
                    for m in members {
                        self.collect_fn_sig(m);
                    }
                }
            }
            TopLevelDecl::Trait { items, .. } => {
                for item in items {
                    if let TraitItem::Method(sig) = item {
                        self.collect_fn_sig_only(sig);
                    }
                }
            }
            TopLevelDecl::Const { name, ty, .. } => {
                if let (Some(def), Some(t)) = (
                    self.find_def(self.current_module, name.symbol, DefKind::Const),
                    ty.as_ref(),
                ) {
                    let tid = self.lower_ast_type(t);
                    self.value_types.insert(def, tid);
                }
            }
            TopLevelDecl::Var { name, ty, .. } => {
                if let Some(def) = self.find_def(self.current_module, name.symbol, DefKind::Var) {
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
        if let Some(def) = self.find_def(self.current_module, f.name.symbol, DefKind::Fn) {
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
        if let Some(def) = self.find_def(self.current_module, sig.name.symbol, DefKind::Fn) {
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
            TopLevelDecl::Impl {
                type_name, members, ..
            } => {
                if let Some(&type_def) = self.type_defs.get(&type_name.symbol) {
                    let self_ty = self.types.intern(&Ty::Named {
                        def: type_def,
                        args: vec![],
                    });
                    self.impl_self_type = Some(self_ty);
                    for m in members {
                        self.check_function(m);
                    }
                    self.impl_self_type = None;
                } else {
                    for m in members {
                        self.check_function(m);
                    }
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
        let expr_start = self.next_expr;
        let has_receiver = f.params.iter().any(|p| matches!(p, Param::Receiver { .. }));
        for p in &f.params {
            match p {
                Param::Named { name, ty, .. } => {
                    let pty = self.lower_ast_type(ty);
                    self.define_local(name.symbol, pty, BindingKind::Param);
                }
                Param::Receiver { ty, .. } => {
                    let pty = ty
                        .as_ref()
                        .map(|t| self.lower_ast_type(t))
                        .or(self.impl_self_type)
                        .unwrap_or(self.unit);
                    self.define_local(impl_receiver_symbol(), pty, BindingKind::Param);
                }
                _ => {}
            }
        }
        if self.impl_self_type.is_some() && !has_receiver {
            if let Some(self_ty) = self.impl_self_type {
                self.define_local(impl_receiver_symbol(), self_ty, BindingKind::Param);
            }
        }
        let body_ty = self.check_block_value(&f.body.inner);
        if body_ty != ret {
            self.error_mismatch(ret, body_ty, f.body.span);
        }
        if let Some(mut builder) = self.layout.take() {
            builder.set_expr_range(expr_start, self.next_expr);
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
                self.check_pattern(&pattern.inner, s, pattern.span);
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
                    let name = self.resolved.interner.resolve(ident.symbol).to_owned();
                    self.bag.push(TypeCheckError::MovedAssignTarget {
                        name,
                        move_span,
                        span: target.span,
                    });
                }
                self.check_ident(ident, target.span)
            }
            Expr::Postfix { base, ops } if ops.len() == 1 => {
                let base_ty = if matches!(ops[0], PostfixOp::Field(_)) {
                    self.check_expr_node_read(base)
                } else {
                    self.check_expr_node(base)
                };
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
        self.check_expr_node_inner(expr, true)
    }

    fn check_expr_node_read(&mut self, expr: &ExprNode) -> TypeId {
        self.check_expr_node_inner(expr, false)
    }

    fn check_expr_node_inner(&mut self, expr: &ExprNode, record_move: bool) -> TypeId {
        let id = self.alloc_expr_id();
        let ty = self.check_expr_with_move(&expr.inner, expr.span, record_move);
        self.expr_types.insert(id, ty);
        ty
    }

    fn check_expr_with_move(&mut self, expr: &Expr, span: Span, record_move: bool) -> TypeId {
        match expr {
            Expr::Ident(ident) => self.check_ident_inner(ident, span, record_move),
            Expr::Postfix { base, ops } => {
                self.check_postfix_with_move(base, ops, span, record_move)
            }
            _ => self.check_expr(expr, span),
        }
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
        self.check_ident_inner(ident, span, true)
    }

    fn check_ident_inner(&mut self, ident: &Ident, span: Span, record_move: bool) -> TypeId {
        if ident.symbol == impl_receiver_symbol() {
            if let Some(ty) = self.ownership.binding_type(ident.symbol) {
                return ty;
            }
        }
        if let Some(move_span) = self.ownership.moved_at(ident.symbol) {
            let name = self.resolved.interner.resolve(ident.symbol).to_owned();
            self.bag.push(TypeCheckError::UseAfterMove {
                name,
                move_span,
                span,
            });
        }
        if let Some(ty) = self.ownership.binding_type(ident.symbol).or_else(|| {
            self.lookup_resolution(span, ident.symbol)
                .and_then(|def| self.value_types.get(&def).copied())
        }) {
            if record_move && !is_copyable(&self.types, ty) {
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
                    if let Some(def) = self.lookup_resolution(span, name.symbol) {
                        if let Some(&fn_ty) = self.value_types.get(&def) {
                            if matches!(self.types.get(fn_ty), Ty::Fn { .. }) {
                                return fn_ty;
                            }
                        }
                    }
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
        self.check_postfix_with_move(base, ops, span, true)
    }

    fn check_postfix_with_move(
        &mut self,
        base: &ExprNode,
        ops: &[PostfixOp],
        span: Span,
        record_move: bool,
    ) -> TypeId {
        let field_only = ops.iter().all(|op| matches!(op, PostfixOp::Field(_)));
        let mut ty = if field_only {
            self.check_expr_node_read(base)
        } else if record_move {
            self.check_expr_node(base)
        } else {
            self.check_expr_node_read(base)
        };
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
        let base = self.deref_for_field(base);
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

    fn deref_for_field(&self, ty: TypeId) -> TypeId {
        match self.types.get(ty) {
            Ty::Ref { inner, .. } => *inner,
            _ => ty,
        }
    }

    fn method_receiver_matches(&self, param: TypeId, receiver: TypeId) -> bool {
        if receiver == param {
            return true;
        }
        if let Ty::Ref { inner, .. } = self.types.get(param) {
            return receiver == *inner;
        }
        false
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
            Ty::Tuple(elems) if !elems.is_empty() => elems[0],
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
        if let Ty::Named { def, .. } = self.types.get(receiver) {
            let def = *def;
            let fn_def = self
                .program_layout
                .inherent_methods
                .get(&(def, name.symbol))
                .copied()
                .or_else(|| find_trait_method_def(&self.program_layout, def, name.symbol));
            if let Some(fn_def) = fn_def {
                if let Some(&fn_ty) = self.value_types.get(&fn_def) {
                    let Ty::Fn { params, ret } = self.types.get(fn_ty).clone() else {
                        return self.unit;
                    };
                    if params.is_empty() {
                        return self.check_call(fn_ty, args, span);
                    }
                    if !self.method_receiver_matches(params[0], receiver) {
                        self.error_mismatch(params[0], receiver, span);
                    }
                    let rest = &params[1..];
                    if rest.len() != args.len() {
                        self.bag.push(TypeCheckError::ArityMismatch {
                            expected: rest.len(),
                            found: args.len(),
                            span,
                        });
                    }
                    for (p, arg) in rest.iter().zip(args) {
                        let got = self.check_expr_node(arg);
                        if got != *p {
                            self.error_mismatch(*p, got, arg.span);
                        }
                    }
                    return ret;
                }
            }
            let mut trait_matches: Vec<DefId> = self
                .program_layout
                .trait_methods
                .iter()
                .filter(|((type_def, _trait_def, method), _)| {
                    *type_def == def && *method == name.symbol
                })
                .map(|(_, fn_def)| *fn_def)
                .collect();
            trait_matches.sort_by_key(|d| d.index());
            trait_matches.dedup();
            if trait_matches.len() > 1 {
                self.bag.push(TypeCheckError::AmbiguousMethod {
                    receiver: self.format_ty(receiver),
                    method_index: name.symbol.index(),
                    span,
                });
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
        if let Some(layout) = &mut self.layout {
            let _ = layout.alloc_match_scrutinee_temp(s);
        }
        let mut acc: Option<TypeId> = None;
        for arm in arms {
            self.check_pattern(&arm.pattern.inner, s, arm.pattern.span);
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

    fn scrutinee_enum_def(&self, scrutinee: TypeId) -> Option<DefId> {
        match self.types.get(scrutinee) {
            Ty::Named { def, .. } if self.program_layout.enums.contains_key(def) => Some(*def),
            _ => None,
        }
    }

    fn error_enum_pattern_on_non_enum(&mut self, scrutinee: TypeId, span: Span) {
        self.bag.push(TypeCheckError::Mismatch {
            expected: "enum".to_string(),
            found: self.format_ty(scrutinee),
            span,
        });
    }

    fn error_enum_variant_mismatch(&mut self, expected_def: DefId, scrutinee: TypeId, span: Span) {
        self.bag.push(TypeCheckError::Mismatch {
            expected: self.format_named(expected_def),
            found: self.format_ty(scrutinee),
            span,
        });
    }

    fn check_pattern(&mut self, pat: &Pattern, scrutinee: TypeId, span: Span) {
        match pat {
            Pattern::Wildcard | Pattern::Literal(_) => {}
            Pattern::Ident(ident) => {
                if let Some((variant_enum_def, _variant)) =
                    self.program_layout.enum_variant_by_name(ident.symbol)
                {
                    if let Some(scrutinee_enum) = self.scrutinee_enum_def(scrutinee) {
                        if scrutinee_enum != variant_enum_def {
                            self.error_enum_variant_mismatch(variant_enum_def, scrutinee, span);
                        }
                    } else {
                        self.error_enum_pattern_on_non_enum(scrutinee, span);
                    }
                } else {
                    self.define_local(ident.symbol, scrutinee, BindingKind::Var);
                }
            }
            Pattern::Struct { name, fields } => {
                if let Some(&def) = self.type_defs.get(&name.symbol) {
                    if let Ty::Named { def: sdef, .. } = self.types.get(scrutinee) {
                        if *sdef != def {
                            self.bag.push(TypeCheckError::Mismatch {
                                expected: self.format_named(def),
                                found: self.format_ty(scrutinee),
                                span,
                            });
                        }
                    }
                    for field in fields {
                        if let Some(fty) = self
                            .struct_fields
                            .get(&def)
                            .and_then(|sf| sf.fields.get(&field.name.symbol).copied())
                        {
                            if let Some(p) = &field.pattern {
                                self.check_pattern(&p.inner, fty, span);
                            } else {
                                self.define_local(field.name.symbol, fty, BindingKind::Var);
                            }
                        }
                    }
                } else if let Some((enum_def, variant)) =
                    self.program_layout.enum_variant_by_name(name.symbol)
                {
                    if let Some(scrutinee_enum) = self.scrutinee_enum_def(scrutinee) {
                        if scrutinee_enum != enum_def {
                            self.error_enum_variant_mismatch(enum_def, scrutinee, span);
                        }
                    } else {
                        self.error_enum_pattern_on_non_enum(scrutinee, span);
                    }
                    if let VariantKind::Struct(payload) = &variant.kind {
                        let field_map: HashMap<Symbol, TypeId> = payload.iter().copied().collect();
                        for field in fields {
                            if let Some(fty) = field_map.get(&field.name.symbol) {
                                if let Some(p) = &field.pattern {
                                    self.check_pattern(&p.inner, *fty, span);
                                } else {
                                    self.define_local(field.name.symbol, *fty, BindingKind::Var);
                                }
                            }
                        }
                    }
                }
            }
            Pattern::Tuple { name, patterns } => {
                if let Some((enum_def, variant)) =
                    self.program_layout.enum_variant_by_name(name.symbol)
                {
                    if let Some(scrutinee_enum) = self.scrutinee_enum_def(scrutinee) {
                        if scrutinee_enum != enum_def {
                            self.error_enum_variant_mismatch(enum_def, scrutinee, span);
                        }
                    } else {
                        self.error_enum_pattern_on_non_enum(scrutinee, span);
                    }
                    if let VariantKind::Tuple(payload) = &variant.kind {
                        for (p, pty) in patterns.iter().zip(payload.iter()) {
                            self.check_pattern(&p.inner, *pty, span);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn format_named(&self, def: DefId) -> String {
        self.resolved
            .defs
            .get(def.index() as usize)
            .map(|d| format!("type#{}", d.name.index()))
            .unwrap_or_else(|| "<?>".to_string())
    }

    fn check_struct_lit(
        &mut self,
        name: &TypeName,
        fields: &[StructFieldInit],
        span: Span,
    ) -> TypeId {
        if let Some(&def) = self.type_defs.get(&name.symbol) {
            let ty = self.types.intern(&Ty::Named { def, args: vec![] });
            if let Some(sl) = self.program_layout.structs.get(&def) {
                let required_fields: Vec<Symbol> = sl.fields.iter().map(|(n, _)| *n).collect();
                for field in fields {
                    if matches!(field, StructFieldInit::Spread(_)) {
                        self.bag.push(TypeCheckError::UnsupportedFeature {
                            feature: "struct literal spread",
                            span,
                        });
                    }
                }
                let mut seen = std::collections::HashSet::new();
                for field in fields {
                    let StructFieldInit::Field { name: fname, value } = field else {
                        continue;
                    };
                    seen.insert(fname.symbol);
                    if let Some(expected) = self
                        .struct_fields
                        .get(&def)
                        .and_then(|sf| sf.fields.get(&fname.symbol).copied())
                    {
                        let got = self.check_expr_node(value);
                        if got != expected {
                            self.error_mismatch(expected, got, value.span);
                        }
                    } else {
                        self.bag.push(TypeCheckError::UnsupportedFeature {
                            feature: "unknown struct field",
                            span: value.span,
                        });
                    }
                }
                for fname in required_fields {
                    if !seen.contains(&fname) {
                        self.bag.push(TypeCheckError::UnsupportedFeature {
                            feature: "missing struct field",
                            span,
                        });
                    }
                }
            }
            return ty;
        }
        if let Some((enum_def, variant)) = self.program_layout.enum_variant_by_name(name.symbol) {
            let enum_ty = self.types.intern(&Ty::Named {
                def: enum_def,
                args: vec![],
            });
            if let VariantKind::Struct(payload) = &variant.kind {
                let field_map: HashMap<Symbol, TypeId> = payload.iter().copied().collect();
                let required_fields: Vec<Symbol> = payload.iter().map(|(n, _)| *n).collect();
                for field in fields {
                    if matches!(field, StructFieldInit::Spread(_)) {
                        self.bag.push(TypeCheckError::UnsupportedFeature {
                            feature: "enum struct literal spread",
                            span,
                        });
                    }
                }
                let mut seen = std::collections::HashSet::new();
                for field in fields {
                    let StructFieldInit::Field { name: fname, value } = field else {
                        continue;
                    };
                    seen.insert(fname.symbol);
                    if let Some(expected) = field_map.get(&fname.symbol) {
                        let got = self.check_expr_node(value);
                        if got != *expected {
                            self.error_mismatch(*expected, got, value.span);
                        }
                    } else {
                        self.bag.push(TypeCheckError::UnsupportedFeature {
                            feature: "unknown enum variant field",
                            span: value.span,
                        });
                    }
                }
                for fname in required_fields {
                    if !seen.contains(&fname) {
                        self.bag.push(TypeCheckError::UnsupportedFeature {
                            feature: "missing enum variant field",
                            span,
                        });
                    }
                }
            }
            return enum_ty;
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
        ProgramLayout,
    ) {
        (
            self.types,
            self.expr_types,
            self.bag,
            self.functions,
            self.program_layout,
        )
    }
}

fn loop_control_stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Break(Some(e)) => e.span,
        Stmt::Break(None) | Stmt::Continue => Span::new(0, 0),
        _ => Span::new(0, 0),
    }
}

fn find_trait_method_def(layout: &ProgramLayout, type_def: DefId, method: Symbol) -> Option<DefId> {
    let mut matches: Vec<DefId> = layout
        .trait_methods
        .iter()
        .filter(|((t, _, m), _)| *t == type_def && *m == method)
        .map(|(_, f)| *f)
        .collect();
    matches.sort_by_key(|d| d.index());
    matches.dedup();
    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
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
    let (types, expr_types, bag, functions, layout) = checker.finish();
    if bag.has_errors() {
        return Err(bag);
    }
    Ok(super::TypedProgram {
        resolved: resolved.clone(),
        types,
        expr_types,
        functions,
        entry: resolved.main_fn,
        layout,
    })
}

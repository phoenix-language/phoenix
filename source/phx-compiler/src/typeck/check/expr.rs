//! Expression type checking, calls, and postfix operators.

use std::collections::HashMap;

use phx_diagnostics::{MismatchKind, Span, TypeCheckError};
use phx_syntax::ast::decl::{Function, ImplMember, Param, TopLevelDecl, TraitItem};
use phx_syntax::ast::expr::{BinOp, Expr, IfCondition, PostfixOp, UnaryOp};
use phx_syntax::ast::ident::{Ident, Path, PathSegment, TypeName};
use phx_syntax::ast::lit::Literal;
use phx_syntax::ast::types::GenericParam;
use phx_syntax::ast::types::Type;
use phx_syntax::ast::{BlockNode, ExprNode, Node};
use phx_syntax::{Symbol, impl_receiver_symbol};

use super::TypeChecker;
use super::decl::generic_bounds_in_decl;
use super::impls::find_trait_method_def;
use crate::resolver::{DefId, DefKind};
use crate::typeck::MethodCallSiteMeta;
use crate::typeck::PrimitiveMethodSite;
use crate::typeck::bindings::BindingKind;
use crate::typeck::bounds::{
    resolve_from_fn_for_error, trait_bound_head, validate_instantiation_bounds,
};
use crate::typeck::builtins::{float_literal_type, int_literal_type, str_type, u8_type};
use crate::typeck::infer::InferenceCtx;
use crate::typeck::intrinsic_kernel::IntrinsicSite;
use crate::typeck::layout::{TraitInstKey, TypeMonoKey, VariantKind};
use crate::typeck::lower_ty::TypeDefMap;
use crate::typeck::mono::{
    MonoInst, TypeMonoInst, TypeMonoKind, generic_param_defs_for_type, generic_params_for_def,
};
use crate::typeck::ops::{check_binary, check_cast, check_unary};
use crate::typeck::ownership::OwnershipTracker;
use crate::typeck::std_kernel::{TryFailureMode, TrySiteMeta};
use crate::typeck::subst::Substitution;
use crate::typeck::types::{ExprId, Ty, TypeId};
use crate::typeck::unify::unify_branch;
use phx_syntax::token::Keyword;

impl TypeChecker<'_> {
    pub(in crate::typeck::check) fn symbol_named(&self, name: &str) -> Option<Symbol> {
        let interner = &self.resolved.interner;
        self.program_layout
            .trait_assoc_impls
            .keys()
            .map(|(_, sym)| *sym)
            .chain(
                self.program_layout
                    .trait_methods
                    .keys()
                    .map(|(_, sym)| *sym),
            )
            .chain(
                self.program_layout
                    .inherent_methods
                    .keys()
                    .map(|(_, sym)| *sym),
            )
            .find(|sym| interner.resolves_to(*sym, name))
            .or_else(|| {
                self.resolved
                    .defs
                    .iter()
                    .find_map(|d| interner.resolves_to(d.name, name).then_some(d.name))
            })
    }

    pub(in crate::typeck::check) fn resolve_trait_def_by_name(&self, name: &str) -> Option<DefId> {
        self.std_trait_kernel
            .trait_def_for_name(&self.resolved.interner, name)
    }

    pub(in crate::typeck::check) fn option_ty_for_item(
        &mut self,
        item_ty: TypeId,
    ) -> Option<TypeId> {
        let option_def = self.std_kernel.option_enum?;
        Some(self.types.intern(&Ty::Named {
            def: option_def,
            args: vec![item_ty],
        }))
    }

    pub(in crate::typeck::check) fn some_variant_symbol(
        &self,
        _option_ty: TypeId,
    ) -> Option<Symbol> {
        let v = self.std_kernel.some_variant?;
        Some(self.resolved.defs.get(v.index() as usize)?.name)
    }
    pub(in crate::typeck::check) fn check_assign_expr(
        &mut self,
        target: &ExprNode,
        value: &ExprNode,
        span: Span,
    ) -> TypeId {
        let lhs = self.check_assign_target(target);
        let rhs = self.check_expr_node(value);
        if !self.types_equal(lhs, rhs) {
            let name = if let Expr::Ident(ident) = &target.inner {
                Some(self.symbol_name(ident.symbol))
            } else {
                None
            };
            self.error_mismatch(lhs, rhs, span, MismatchKind::Assign { name });
        }
        if let Expr::Ident(ident) = &value.inner {
            if !self.is_copyable_ty(rhs) {
                self.ownership.move_binding(ident.symbol, value.span);
            }
        }
        rhs
    }

    pub(in crate::typeck::check) fn check_assign_target(&mut self, target: &ExprNode) -> TypeId {
        match &target.inner {
            Expr::Ident(ident) => {
                if let Some(move_span) = self.ownership.moved_at(ident.symbol) {
                    let name = self.symbol_name(ident.symbol);
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::MovedAssignTarget {
                            name,
                            move_span,
                            span: target.span,
                        },
                    );
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
                } else if let PostfixOp::Index(idx) = &ops[0] {
                    let _ = self.check_expr_node(idx);
                    self.check_index(base_ty, target.span)
                } else {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::InvalidOperator {
                            op: "assign target",
                            span: target.span,
                        },
                    );
                    self.unit
                }
            }
            Expr::Unary {
                op: phx_syntax::ast::expr::UnaryOp::Deref,
                operand,
                ..
            } => {
                let ptr_ty = self.check_expr_node_read(operand);
                if let Ty::Ptr { inner, .. } = self.types.get(ptr_ty) {
                    *inner
                } else {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::InvalidOperator {
                            op: "assign through deref",
                            span: target.span,
                        },
                    );
                    self.unit
                }
            }
            _ => {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::InvalidOperator {
                        op: "assign target",
                        span: target.span,
                    },
                );
                self.unit
            }
        }
    }

    pub(in crate::typeck::check) fn move_if_non_copyable(&mut self, init: &ExprNode, ty: TypeId) {
        if self.is_copyable_ty(ty) {
            return;
        }
        if let Expr::Ident(ident) = &init.inner {
            self.ownership.move_binding(ident.symbol, init.span);
        }
    }

    pub(in crate::typeck::check) fn check_expr_node(&mut self, expr: &ExprNode) -> TypeId {
        self.check_expr_node_inner(expr, true, true)
    }

    pub(in crate::typeck::check) fn check_expr_node_read(&mut self, expr: &ExprNode) -> TypeId {
        self.check_expr_node_inner(expr, false, true)
    }

    /// Types `expr` for generic inference only — does not allocate a lowering cursor id.
    pub(in crate::typeck::check) fn check_expr_node_infer(&mut self, expr: &ExprNode) -> TypeId {
        self.check_expr_node_inner(expr, false, false)
    }

    pub(in crate::typeck::check) fn check_expr_node_inner(
        &mut self,
        expr: &ExprNode,
        record_move: bool,
        record_expr_id: bool,
    ) -> TypeId {
        let id = if record_expr_id {
            self.alloc_expr_id()
        } else {
            ExprId::from_raw(0)
        };
        let ty = self.check_expr_with_move(&expr.inner, expr.span, record_move, id);
        if record_expr_id {
            self.expr_types.insert(id, ty);
            self.expr_span_types
                .insert((self.current_module, expr.span), ty);
        }
        ty
    }

    pub(in crate::typeck::check) fn check_expr_with_move(
        &mut self,
        expr: &Expr,
        span: Span,
        record_move: bool,
        expr_id: ExprId,
    ) -> TypeId {
        match expr {
            Expr::Ident(ident) => self.check_ident_inner(ident, span, record_move),
            Expr::Postfix { base, ops } => {
                self.check_postfix_with_move(base, ops, span, record_move, expr_id)
            }
            Expr::Literal(_)
            | Expr::Path(_)
            | Expr::Tuple(_)
            | Expr::Array(_)
            | Expr::Unary { .. }
            | Expr::Binary { .. }
            | Expr::Assign { .. }
            | Expr::Cast { .. }
            | Expr::If { .. }
            | Expr::Match { .. }
            | Expr::Block(_)
            | Expr::StructLit { .. }
            | Expr::Unsafe(_)
            | Expr::Range { .. }
            | Expr::Lambda { .. }
            | Expr::RuntimeDirective { .. } => self.check_expr(expr, span),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(in crate::typeck::check) fn check_expr(&mut self, expr: &Expr, span: Span) -> TypeId {
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
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::InvalidOperator { op: "unary", span },
                    );
                    self.unit
                })
            }
            Expr::Binary { op, left, right } => {
                if matches!(op, BinOp::Pow) {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnsupportedFeature {
                            feature: "integer power (`**`)",
                            span,
                        },
                    );
                    return self.unit;
                }
                let l = self.check_expr_node(left);
                let r = self.check_expr_node(right);
                check_binary(&mut self.types, *op, l, r)
                    .map(|r| r.result)
                    .unwrap_or_else(|| {
                        self.bag.push(
                            self.current_module,
                            TypeCheckError::InvalidOperator { op: "binary", span },
                        );
                        self.unit
                    })
            }
            Expr::Assign { target, value, .. } => self.check_assign_expr(target, value, span),
            Expr::Cast { expr, ty } => {
                let from = self.check_expr_node(expr);
                let to = self.lower_ast_type(ty);
                if !check_cast(&self.alias_env(), from, to)
                    && !self.check_tuple_struct_cast(from, to)
                    && !self.check_utf8_array_to_str_cast(from, to, &expr.inner)
                {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::InvalidCast {
                            from: self.format_ty(from),
                            to: self.format_ty(to),
                            span,
                        },
                    );
                }
                to
            }
            Expr::Postfix { base, ops } => self.check_postfix(base, ops, span),
            Expr::If {
                condition,
                then_block,
                else_ifs,
                else_block,
            } => self.check_if(condition.as_ref(), then_block, else_ifs, else_block, span),
            Expr::Match { scrutinee, arms } => self.check_match(scrutinee, arms, span),
            Expr::Block(block) => self.check_block_expr(block),
            Expr::StructLit {
                name,
                generics,
                fields,
            } => self.check_struct_lit(name, generics.as_deref(), fields, span),
            Expr::Unsafe(block) => {
                let mut result = self.unit;
                self.with_unsafe(|this| {
                    result = this.check_block_expr(block);
                });
                result
            }
            Expr::Range { start, end, .. } => {
                let _ = self.check_expr_node(start);
                let _ = self.check_expr_node(end);
                self.push_unsupported("range expression", span);
                self.unit
            }
            Expr::Lambda { body, .. } => {
                let body_span = match body {
                    phx_syntax::ast::expr::LambdaBody::Expr(e) => e.span,
                    phx_syntax::ast::expr::LambdaBody::Block(b) => b.span,
                };
                self.push_unsupported("lambda expression", body_span);
                self.unit
            }
            Expr::RuntimeDirective { kind, args, .. } => {
                let feature = match kind {
                    phx_syntax::ast::expr::RuntimeDirectiveKind::Spawn => "@spawn directive",
                    phx_syntax::ast::expr::RuntimeDirectiveKind::Send => "@send directive",
                    phx_syntax::ast::expr::RuntimeDirectiveKind::Receive => "@receive directive",
                    phx_syntax::ast::expr::RuntimeDirectiveKind::Reply => "@reply directive",
                };
                for arg in args {
                    let _ = self.check_expr_node(arg);
                }
                self.push_unsupported(feature, span);
                self.unit
            }
        }
    }

    pub(in crate::typeck::check) fn check_block_expr(&mut self, block: &BlockNode) -> TypeId {
        self.check_block_value(&block.inner)
    }

    pub(in crate::typeck::check) fn check_literal(&mut self, lit: &Literal) -> TypeId {
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
            Literal::String(_) => str_type(&mut self.types),
        }
    }

    pub(in crate::typeck::check) fn check_ident(&mut self, ident: &Ident, span: Span) -> TypeId {
        self.check_ident_inner(ident, span, true)
    }

    pub(in crate::typeck::check) fn check_ident_inner(
        &mut self,
        ident: &Ident,
        span: Span,
        record_move: bool,
    ) -> TypeId {
        if ident.symbol == impl_receiver_symbol() {
            if let Some(ty) = self.ownership.binding_type(ident.symbol) {
                return ty;
            }
        }
        if let Some(move_span) = self.ownership.moved_at(ident.symbol) {
            let name = self.symbol_name(ident.symbol);
            self.bag.push(
                self.current_module,
                TypeCheckError::UseAfterMove {
                    name,
                    move_span,
                    span,
                },
            );
        }
        if let Some(ty) = self.ownership.binding_type(ident.symbol).or_else(|| {
            self.lookup_resolution(ident.id)
                .and_then(|def| self.value_types.get(&def).copied())
        }) {
            if record_move && !self.is_copyable_ty(ty) {
                self.ownership.move_binding(ident.symbol, span);
            }
            return ty;
        }
        if let Some(def) = self.lookup_resolution(ident.id) {
            if let Some(fields) = self.struct_fields.get(&def) {
                let _ = fields;
            }
        }
        self.bag.push(
            self.current_module,
            TypeCheckError::UnresolvedValue {
                symbol_index: ident.symbol.index(),
                span,
            },
        );
        self.unit
    }

    pub(in crate::typeck::check) fn check_path(&mut self, path: &Path, span: Span) -> TypeId {
        if path.segments.len() == 1 {
            match &path.segments[0] {
                PathSegment::Ident(ident) => return self.check_ident(ident, span),
                PathSegment::Type(seg) => {
                    if let Some(def) = self.lookup_resolution(seg.name.id) {
                        if let Some(&fn_ty) = self.value_types.get(&def) {
                            if matches!(self.types.get(fn_ty), Ty::Fn { .. }) {
                                return fn_ty;
                            }
                        }
                    }
                    if let Some(def) = self.type_defs.get(&seg.name.symbol).copied() {
                        return self.value_types.get(&def).copied().unwrap_or_else(|| {
                            self.types.intern(&Ty::Named { def, args: vec![] })
                        });
                    }
                }
            }
        }
        self.unit
    }

    pub(in crate::typeck::check) fn associated_fn_target(
        &mut self,
        base: &ExprNode,
    ) -> Option<(TypeId, Ident)> {
        let Expr::Path(path) = &base.inner else {
            return None;
        };
        if path.segments.len() != 2 {
            return None;
        }
        let method = match &path.segments[1] {
            PathSegment::Ident(ident) => *ident,
            PathSegment::Type(_) => return None,
        };
        let target_ty = self.resolve_type_segment_for_assoc_fn(&path.segments[0])?;
        Some((target_ty, method))
    }

    pub(in crate::typeck::check) fn resolve_type_segment_for_assoc_fn(
        &mut self,
        segment: &PathSegment,
    ) -> Option<TypeId> {
        match segment {
            PathSegment::Type(seg) => {
                let def = if let Some(def) = self.lookup_resolution(seg.name.id) {
                    if self
                        .resolved
                        .defs
                        .get(def.index() as usize)
                        .is_some_and(|d| {
                            matches!(d.kind, DefKind::Struct | DefKind::Enum | DefKind::TypeAlias)
                        })
                    {
                        def
                    } else {
                        self.type_defs.get(&seg.name.symbol).copied()?
                    }
                } else {
                    self.type_defs.get(&seg.name.symbol).copied()?
                };
                let span = seg.name.span;
                if let Some(generic_nodes) = &seg.generics {
                    let mut provided: Vec<TypeId> = generic_nodes
                        .iter()
                        .map(|n| self.lower_ast_type(n))
                        .collect();
                    if let Some(completed) = self.complete_generic_args(def, provided, span) {
                        provided = completed;
                    } else {
                        return Some(self.poison_type());
                    }
                    return Some(self.resolve_instantiated_named(def, provided, span));
                }
                Some(self.types.intern(&Ty::Named { def, args: vec![] }))
            }
            PathSegment::Ident(ident) => {
                if let Some(subst) = &self.subst {
                    if let Some(def) = self.type_defs.get(&ident.symbol).copied() {
                        if let Some(concrete) = subst.get(def) {
                            return Some(concrete);
                        }
                    }
                }
                if let Some(def) = self.lookup_resolution(ident.id) {
                    if let Some(subst) = &self.subst {
                        if let Some(concrete) = subst.get(def) {
                            return Some(concrete);
                        }
                    } else if self
                        .resolved
                        .defs
                        .get(def.index() as usize)
                        .is_some_and(|d| d.kind == DefKind::GenericParam)
                    {
                        return Some(self.types.intern(&Ty::Named { def, args: vec![] }));
                    }
                }
                let def = self.type_defs.get(&ident.symbol).copied()?;
                if let Some(subst) = &self.subst {
                    if let Some(concrete) = subst.get(def) {
                        return Some(concrete);
                    }
                }
                Some(self.types.intern(&Ty::Named { def, args: vec![] }))
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(in crate::typeck::check) fn check_associated_fn_call(
        &mut self,
        target_ty: TypeId,
        method: Ident,
        generics: Option<&[Node<Type>]>,
        args: &[ExprNode],
        span: Span,
        site_id: ExprId,
    ) -> TypeId {
        let Ty::Named {
            def,
            args: implementer_args,
        } = self.types.get(target_ty).clone()
        else {
            self.bag.push(
                self.current_module,
                TypeCheckError::UnresolvedMethod {
                    receiver: self.format_ty(target_ty),
                    method_index: method.symbol.index(),
                    span,
                },
            );
            return self.unit;
        };
        let Some(fn_def) = self
            .program_layout
            .inherent_methods
            .get(&(def, method.symbol))
            .copied()
            .or_else(|| {
                find_trait_method_def(&self.program_layout, def, &implementer_args, method.symbol)
            })
        else {
            self.emit_ambiguous_or_unresolved_method(target_ty, def, &method, span);
            return self.unit;
        };
        self.associated_fn_sites.insert(site_id, fn_def);
        self.check_unsafe_fn_call(fn_def, span);
        let Some(&fn_ty) = self.value_types.get(&fn_def) else {
            return self.unit;
        };
        let Some(f) = self.find_function_decl(fn_def).cloned() else {
            return self.check_call(fn_ty, args, span);
        };
        let impl_param_defs = self.impl_generic_param_defs(def);
        let method_param_defs = self.generic_param_defs_for_fn(&f, fn_def);
        let Ty::Fn { params, ret } = self.types.get(fn_ty).clone() else {
            return self.unit;
        };
        let mut impl_args = implementer_args.clone();
        if impl_args.len() < impl_param_defs.len() {
            if let Some(call_generics) = generics {
                let type_defs = self.type_defs.clone();
                let partial: Vec<TypeId> = call_generics
                    .iter()
                    .map(|n| self.lower_ast_type_with_defs(n, &type_defs))
                    .collect();
                if let Some(filled) = self.complete_partial_generic_args_with_inference(
                    def,
                    &impl_param_defs,
                    partial,
                    &params,
                    args,
                    span,
                ) {
                    impl_args = filled;
                } else {
                    return self.unit;
                }
            } else if let Some(filled) = self.complete_generic_args(def, impl_args, span) {
                impl_args = filled;
            } else {
                return self.unit;
            }
        }
        if impl_param_defs.len() != impl_args.len() && !impl_param_defs.is_empty() {
            self.bag.push(
                self.current_module,
                TypeCheckError::Mismatch {
                    expected: "fully instantiated type".to_owned(),
                    found: self.format_ty(target_ty),
                    span,
                    kind: MismatchKind::default(),
                },
            );
            return self.unit;
        }
        let needs_mono = !impl_param_defs.is_empty() || !method_param_defs.is_empty();
        if needs_mono {
            let mut subst = Substitution::new();
            for (param_def, concrete) in impl_param_defs.iter().zip(&impl_args) {
                subst.insert(*param_def, *concrete);
            }
            let infer_params: Vec<_> = params
                .iter()
                .map(|p| Substitution::apply(&mut self.types, *p, &subst, self.resolved))
                .collect();
            let call_generics_for_method = if method_param_defs.is_empty() {
                None
            } else {
                generics
            };
            let method_args = if method_param_defs.is_empty() {
                Vec::new()
            } else {
                let Some(concrete) = self.resolve_concrete_generic_args(
                    call_generics_for_method,
                    f.generics.as_deref(),
                    &method_param_defs,
                    &infer_params,
                    args,
                    span,
                    None,
                    self.def_module(fn_def),
                ) else {
                    return self.unit;
                };
                concrete
            };
            if generics.is_some() && method_param_defs.is_empty() && impl_param_defs.is_empty() {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::UnsupportedFeature {
                        feature: "type arguments on non-generic call",
                        span,
                    },
                );
            }
            for (param_def, concrete) in method_param_defs.iter().zip(&method_args) {
                subst.insert(*param_def, *concrete);
            }
            let applied_params: Vec<_> = params
                .iter()
                .map(|p| Substitution::apply(&mut self.types, *p, &subst, self.resolved))
                .collect();
            if applied_params.len() != args.len() {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::ArityMismatch {
                        expected: applied_params.len(),
                        found: args.len(),
                        span,
                    },
                );
            }
            for (index, (p, arg)) in applied_params.iter().zip(args).enumerate() {
                let got = self.check_expr_node(arg);
                if !self.types_equal(got, *p) {
                    self.error_mismatch(*p, got, arg.span, MismatchKind::Argument { index });
                }
            }
            let mut mono_args = impl_args;
            mono_args.extend(method_args);
            self.record_mono_inst(fn_def, mono_args, method.id);
            return Substitution::apply(&mut self.types, ret, &subst, self.resolved);
        }
        if generics.is_some() {
            self.bag.push(
                self.current_module,
                TypeCheckError::UnsupportedFeature {
                    feature: "type arguments on non-generic call",
                    span,
                },
            );
        }
        if params.len() != args.len() {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: params.len(),
                    found: args.len(),
                    span,
                },
            );
        }
        for (index, (p, arg)) in params.iter().zip(args).enumerate() {
            let got = self.check_expr_node(arg);
            if !self.types_equal(got, *p) {
                self.error_mismatch(*p, got, arg.span, MismatchKind::Argument { index });
            }
        }
        ret
    }

    pub(in crate::typeck::check) fn check_postfix(
        &mut self,
        base: &ExprNode,
        ops: &[PostfixOp],
        span: Span,
    ) -> TypeId {
        self.check_postfix_with_move(base, ops, span, true, ExprId::from_raw(0))
    }

    pub(in crate::typeck::check) fn check_postfix_with_move(
        &mut self,
        base: &ExprNode,
        ops: &[PostfixOp],
        span: Span,
        record_move: bool,
        expr_id: ExprId,
    ) -> TypeId {
        let field_only = ops.iter().all(|op| matches!(op, PostfixOp::Field(_)));
        let read_receiver = field_only
            || ops
                .first()
                .is_some_and(|op| matches!(op, PostfixOp::Method { .. }));
        let mut ty = if read_receiver {
            self.check_expr_node_read(base)
        } else if record_move {
            self.check_expr_node(base)
        } else {
            self.check_expr_node_read(base)
        };
        for op in ops {
            ty = match op {
                PostfixOp::Field(name) => self.check_field(ty, name, span),
                PostfixOp::Method {
                    name,
                    generics,
                    args,
                    ..
                } => self.check_method_call_with_generics(
                    Some(base),
                    ty,
                    name,
                    generics.as_deref(),
                    args,
                    span,
                    expr_id,
                ),
                PostfixOp::Call { generics, args } => {
                    let callee_ty = ty;
                    let callee_def = self.callee_def_from_expr(base);
                    if let (Some(def), Some(site)) = (
                        callee_def,
                        callee_def.and_then(|d| self.intrinsic_kernel.site_for_call(d)),
                    ) {
                        if generics.is_some() && site != IntrinsicSite::SizeOf {
                            self.bag.push(
                                self.current_module,
                                TypeCheckError::UnsupportedFeature {
                                    feature: "generic intrinsic call",
                                    span,
                                },
                            );
                        }
                        ty = self.check_intrinsic_call(
                            site,
                            def,
                            generics.as_deref(),
                            args,
                            span,
                            expr_id,
                        );
                    } else {
                        let foreign = callee_def.is_some_and(|d| {
                            self.resolved
                                .defs
                                .get(d.index() as usize)
                                .is_some_and(|rec| rec.kind == DefKind::ExternFn)
                        });
                        if let Some(def) = callee_def {
                            self.check_extern_call(def, span);
                        }
                        ty = if let Some((target_ty, method)) = self.associated_fn_target(base) {
                            self.check_associated_fn_call(
                                target_ty,
                                method,
                                generics.as_deref(),
                                args,
                                span,
                                expr_id,
                            )
                        } else {
                            self.check_call_with_generics(
                                callee_ty,
                                callee_def,
                                base,
                                generics.as_deref(),
                                args,
                                span,
                            )
                        };
                        self.record_indirect_call(callee_ty, callee_def, expr_id, foreign);
                    }
                    ty
                }
                PostfixOp::Index(idx) => {
                    let _ = self.check_expr_node(idx);
                    self.check_index(ty, span)
                }
                PostfixOp::Try => self.check_try_expr(ty, span, expr_id),
            };
        }
        ty
    }

    pub(in crate::typeck::check) fn check_field(
        &mut self,
        base: TypeId,
        field: &Ident,
        span: Span,
    ) -> TypeId {
        let base = self.deref_for_field(base);
        if let Ty::Named { def, args } = self.types.get(base).clone() {
            let field_map = self.struct_fields_for_named(def, &args);
            if let Some(&fty) = field_map.get(&field.symbol) {
                return fty;
            }
        }
        self.bag.push(
            self.current_module,
            TypeCheckError::UnresolvedMethod {
                receiver: self.format_ty(base),
                method_index: field.symbol.index(),
                span,
            },
        );
        self.unit
    }

    pub(in crate::typeck::check) fn deref_for_field(&self, ty: TypeId) -> TypeId {
        match self.types.get(ty) {
            Ty::Ref { inner, .. } => *inner,
            _ => ty,
        }
    }

    pub(in crate::typeck::check) fn callee_def_from_expr(&self, base: &ExprNode) -> Option<DefId> {
        match &base.inner {
            Expr::Ident(ident) => self.lookup_resolution(ident.id),
            Expr::Path(path) if path.segments.len() == 1 => match &path.segments[0] {
                PathSegment::Ident(ident) => self.lookup_resolution(ident.id),
                PathSegment::Type(seg) => self.lookup_resolution(seg.name.id),
            },
            _ => None,
        }
    }

    pub(in crate::typeck::check) fn record_mono_inst(
        &mut self,
        base_fn: DefId,
        args: Vec<TypeId>,
        site: phx_syntax::AstNodeId,
    ) {
        if let Some(inst) = self
            .mono_insts
            .iter_mut()
            .find(|i| i.base_fn == base_fn && i.args == args)
        {
            inst.call_sites.push(site);
            return;
        }
        self.mono_insts.push(MonoInst {
            base_fn,
            args,
            call_sites: vec![site],
            owner_module: self.current_module,
        });
    }

    pub(in crate::typeck::check) fn record_type_mono_inst(
        &mut self,
        base_def: DefId,
        kind: TypeMonoKind,
        args: Vec<TypeId>,
    ) {
        if self
            .type_mono_insts
            .iter()
            .any(|i| i.base_def == base_def && i.kind == kind && i.args == args)
        {
            return;
        }
        self.queue_impl_method_monos(base_def, &args);
        self.type_mono_insts.push(TypeMonoInst {
            base_def,
            kind,
            args,
        });
    }

    pub(in crate::typeck::check) fn queue_impl_method_monos(
        &mut self,
        type_def: DefId,
        args: &[TypeId],
    ) {
        let mut fns: Vec<DefId> = self
            .program_layout
            .inherent_methods
            .iter()
            .filter_map(|((def, _), fn_def)| (*def == type_def).then_some(*fn_def))
            .collect();
        for ((key, _), fn_def) in &self.program_layout.trait_methods {
            if key.implementer == type_def && key.implementer_args.is_empty() {
                fns.push(*fn_def);
            }
        }
        fns.sort_by_key(|d| d.index());
        fns.dedup();
        for base_fn in fns {
            if self
                .find_function_decl(base_fn)
                .is_some_and(|f| f.generics.as_ref().is_some_and(|g| !g.is_empty()))
            {
                continue;
            }
            if self
                .mono_insts
                .iter()
                .any(|i| i.base_fn == base_fn && i.args == args)
            {
                continue;
            }
            if !crate::typeck::mono::fn_instantiation_bounds_hold(
                self.resolved,
                &self.program_layout,
                &mut self.types,
                &self.std_trait_kernel,
                &self.value_types,
                base_fn,
                args,
            ) {
                continue;
            }
            self.mono_insts.push(MonoInst {
                base_fn,
                args: args.to_vec(),
                call_sites: Vec::new(),
                owner_module: self.current_module,
            });
        }
    }

    /// Ensures monomorphized layout exists when matching on a generic enum scrutinee.
    pub(in crate::typeck::check) fn record_scrutinee_type_mono(&mut self, scrutinee: TypeId) {
        let Ty::Named { def, args } = self.types.get(scrutinee).clone() else {
            return;
        };
        if args.is_empty() {
            return;
        }
        let Some(kind) = self.type_mono_kind_for_def(def) else {
            return;
        };
        match kind {
            TypeMonoKind::Enum if self.program_layout.enums.contains_key(&def) => {
                self.record_type_mono_inst(def, kind, args);
            }
            TypeMonoKind::Struct if self.program_layout.structs.contains_key(&def) => {
                self.record_type_mono_inst(def, kind, args);
            }
            _ => {}
        }
    }

    pub(in crate::typeck::check) fn type_mono_kind_for_def(
        &self,
        def: DefId,
    ) -> Option<TypeMonoKind> {
        let record = self.resolved.defs.get(def.index() as usize)?;
        match record.kind {
            DefKind::Struct => Some(TypeMonoKind::Struct),
            DefKind::Enum => Some(TypeMonoKind::Enum),
            DefKind::TypeAlias => Some(TypeMonoKind::Alias),
            _ => None,
        }
    }

    pub(in crate::typeck::check) fn complete_generic_args(
        &mut self,
        base_def: DefId,
        mut provided: Vec<TypeId>,
        span: Span,
    ) -> Option<Vec<TypeId>> {
        let param_defs = generic_param_defs_for_type(self.resolved, base_def).unwrap_or_default();
        if param_defs.is_empty() {
            if provided.is_empty() {
                return Some(provided);
            }
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: 0,
                    found: provided.len(),
                    span,
                },
            );
            return None;
        }
        if provided.len() > param_defs.len() {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: param_defs.len(),
                    found: provided.len(),
                    span,
                },
            );
            return None;
        }
        if provided.len() < param_defs.len() {
            let generic_params = generic_params_for_def(self.resolved, base_def)?;
            let module = self.def_module(base_def);
            let completed = self.with_pushed_generics(module, Some(&generic_params), |this| {
                let type_defs = this.type_defs.clone();
                let mut subst = Substitution::new();
                for (i, param_def) in param_defs.iter().enumerate() {
                    if i < provided.len() {
                        subst.insert(*param_def, provided[i]);
                    }
                }
                let mut result = provided;
                for i in result.len()..param_defs.len() {
                    let param = generic_params.get(i)?;
                    let Some(default) = param.default.as_ref() else {
                        this.bag.push(
                            this.current_module,
                            TypeCheckError::ArityMismatch {
                                expected: param_defs.len(),
                                found: result.len(),
                                span,
                            },
                        );
                        return None;
                    };
                    let lowered = this.lower_ast_type_with_defs(default, &type_defs);
                    let concrete =
                        Substitution::apply(&mut this.types, lowered, &subst, this.resolved);
                    subst.insert(param_defs[i], concrete);
                    result.push(concrete);
                }
                Some(result)
            });
            provided = completed?;
        }
        let bounds_module = self.def_module(base_def);
        if !validate_instantiation_bounds(
            self.resolved,
            &self.program_layout,
            &mut self.types,
            &self.std_trait_kernel,
            &self.value_types,
            generic_params_for_def(self.resolved, base_def).as_deref(),
            &param_defs,
            &provided,
            bounds_module,
            span,
            &mut self.bag,
        ) {
            return None;
        }
        Some(provided)
    }

    pub(in crate::typeck::check) fn complete_generic_args_from_ast(
        &mut self,
        generic_params: Option<&[GenericParam]>,
        param_defs: &[DefId],
        provided_nodes: &[Node<Type>],
        module: u32,
        span: Span,
    ) -> Option<Vec<TypeId>> {
        if provided_nodes.len() > param_defs.len() {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: param_defs.len(),
                    found: provided_nodes.len(),
                    span,
                },
            );
            return None;
        }
        self.with_pushed_generics(module, generic_params, |this| {
            let type_defs = this.type_defs.clone();
            let mut provided: Vec<TypeId> = provided_nodes
                .iter()
                .map(|n| this.lower_ast_type_with_defs(n, &type_defs))
                .collect();
            if provided.len() == param_defs.len() {
                return Some(provided);
            }
            let generic_params = generic_params?;
            let mut subst = Substitution::new();
            for (i, param_def) in param_defs.iter().enumerate() {
                if i < provided.len() {
                    subst.insert(*param_def, provided[i]);
                }
            }
            for i in provided.len()..param_defs.len() {
                let param = generic_params.get(i)?;
                let Some(default) = param.default.as_ref() else {
                    this.bag.push(
                        this.current_module,
                        TypeCheckError::ArityMismatch {
                            expected: param_defs.len(),
                            found: provided.len(),
                            span,
                        },
                    );
                    return None;
                };
                let lowered = this.lower_ast_type_with_defs(default, &type_defs);
                let concrete = Substitution::apply(&mut this.types, lowered, &subst, this.resolved);
                subst.insert(param_defs[i], concrete);
                provided.push(concrete);
            }
            Some(provided)
        })
    }

    pub(in crate::typeck::check) fn resolve_instantiated_named(
        &mut self,
        base: DefId,
        args: Vec<TypeId>,
        span: Span,
    ) -> TypeId {
        let param_defs = generic_param_defs_for_type(self.resolved, base).unwrap_or_default();
        let args: Vec<_> = args
            .into_iter()
            .map(|arg| {
                let arg = if let Some(subst) = &self.subst {
                    Substitution::apply(&mut self.types, arg, subst, self.resolved)
                } else {
                    arg
                };
                self.mono_substitute_generic_param(arg, &[], Some(&param_defs))
            })
            .collect();
        let Some(kind) = self.type_mono_kind_for_def(base) else {
            return self.types.intern(&Ty::Named { def: base, args });
        };
        if param_defs.is_empty() {
            self.bag.push(
                self.current_module,
                TypeCheckError::UnsupportedFeature {
                    feature: "type arguments on non-generic type",
                    span,
                },
            );
            return self.poison_type();
        }
        let Some(args) = self.complete_generic_args(base, args, span) else {
            return self.poison_type();
        };
        self.record_type_mono_inst(base, kind, args.clone());
        if kind == TypeMonoKind::Alias {
            let mut subst = Substitution::new();
            for (param, arg) in param_defs.iter().zip(&args) {
                subst.insert(*param, *arg);
            }
            let Some(&body) = self.value_types.get(&base) else {
                return self.poison_type();
            };
            let expanded = Substitution::apply(&mut self.types, body, &subst, self.resolved);
            self.specialized_aliases
                .insert(TypeMonoKey::new(base, args), expanded);
            return expanded;
        }
        self.types.intern(&Ty::Named { def: base, args })
    }

    pub(in crate::typeck::check) fn struct_fields_for_named(
        &mut self,
        def: DefId,
        args: &[TypeId],
    ) -> HashMap<Symbol, TypeId> {
        let Some(template) = self.struct_fields.get(&def) else {
            return HashMap::new();
        };
        if args.is_empty() {
            return template.fields.clone();
        }
        let param_defs = generic_param_defs_for_type(self.resolved, def).unwrap_or_default();
        let mut subst = Substitution::new();
        for (param, arg) in param_defs.iter().zip(args) {
            subst.insert(*param, *arg);
        }
        if let Some(module) = self
            .resolved
            .defs
            .get(def.index() as usize)
            .map(|d| d.module)
        {
            subst.extend_generic_param_aliases(self.resolved, module);
        }
        template
            .fields
            .iter()
            .map(|(name, ty)| {
                (
                    *name,
                    Substitution::apply(&mut self.types, *ty, &subst, self.resolved),
                )
            })
            .collect()
    }

    pub(in crate::typeck::check) fn check_tuple_struct_cast(
        &mut self,
        from: TypeId,
        to: TypeId,
    ) -> bool {
        if let Some(inner) = self.single_field_tuple_inner(from)
            && self.types_equal(inner, to)
        {
            return true;
        }
        if let Some(inner) = self.single_field_tuple_inner(to)
            && self.types_equal(inner, from)
        {
            return true;
        }
        false
    }

    pub(in crate::typeck::check) fn single_field_tuple_inner(
        &mut self,
        ty: TypeId,
    ) -> Option<TypeId> {
        let Ty::Named { def, args } = self.types.get(ty).clone() else {
            return None;
        };
        if !self.program_layout.tuple_structs.contains(&def) {
            return None;
        }
        let fields: Vec<TypeId> = self
            .struct_fields_for_named(def, &args)
            .into_values()
            .collect();
        if fields.len() != 1 {
            return None;
        }
        fields.into_iter().next()
    }

    pub(in crate::typeck::check) fn tuple_field_symbol(&self, index: usize) -> Option<Symbol> {
        let name = index.to_string();
        self.resolved.interner.lookup(&name)
    }

    pub(in crate::typeck::check) fn enum_def_for_variant(
        &self,
        variant_def: DefId,
    ) -> Option<DefId> {
        self.program_layout
            .variants
            .get(&variant_def)
            .map(|meta| meta.enum_def)
    }

    pub(in crate::typeck::check) fn check_tuple_struct_ctor_call(
        &mut self,
        struct_def: DefId,
        generics: Option<&[Node<phx_syntax::ast::types::Type>]>,
        args: &[ExprNode],
        span: Span,
    ) -> TypeId {
        let param_defs = generic_param_defs_for_type(self.resolved, struct_def).unwrap_or_default();
        let field_types: Vec<TypeId> = self
            .program_layout
            .structs
            .get(&struct_def)
            .map(|sl| sl.fields.iter().map(|(_, ty)| *ty).collect())
            .unwrap_or_default();
        let type_args = if param_defs.is_empty() {
            if generics.is_some() {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::UnsupportedFeature {
                        feature: "type arguments on non-generic tuple struct constructor",
                        span,
                    },
                );
            }
            Vec::new()
        } else {
            let Some(concrete_args) = self.resolve_concrete_generic_args(
                generics,
                generic_params_for_def(self.resolved, struct_def).as_deref(),
                &param_defs,
                &field_types,
                args,
                span,
                Some(struct_def),
                self.def_module(struct_def),
            ) else {
                return self.unit;
            };
            self.record_type_mono_inst(struct_def, TypeMonoKind::Struct, concrete_args.clone());
            concrete_args
        };
        let struct_ty = self.types.intern(&Ty::Named {
            def: struct_def,
            args: type_args.clone(),
        });
        let params = self.struct_fields_for_named(struct_def, &type_args);
        let param_types: Vec<TypeId> = self
            .program_layout
            .structs
            .get(&struct_def)
            .map(|sl| {
                sl.fields
                    .iter()
                    .map(|(name, _)| *params.get(name).unwrap_or(&self.unit))
                    .collect()
            })
            .unwrap_or_default();
        if param_types.len() != args.len() {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: param_types.len(),
                    found: args.len(),
                    span,
                },
            );
        }
        for (index, (p, arg)) in param_types.iter().zip(args.iter()).enumerate() {
            let got = self.check_expr_node(arg);
            if !self.types_equal(got, *p) {
                self.error_mismatch(*p, got, arg.span, MismatchKind::Argument { index });
            }
        }
        struct_ty
    }

    pub(in crate::typeck::check) fn substituted_variant_payload(
        &mut self,
        variant_def: DefId,
        args: &[TypeId],
    ) -> Option<VariantKind> {
        let meta = self.program_layout.variants.get(&variant_def)?.clone();
        if args.is_empty() {
            return Some(meta.payload);
        }
        let enum_def = meta.enum_def;
        let param_defs = generic_param_defs_for_type(self.resolved, enum_def)?;
        let mut subst = Substitution::new();
        for (param, arg) in param_defs.iter().zip(args) {
            subst.insert(*param, *arg);
        }
        Some(match meta.payload {
            VariantKind::Unit => VariantKind::Unit,
            VariantKind::Tuple(ts) => {
                let pts: Vec<_> = ts
                    .iter()
                    .map(|t| Substitution::apply(&mut self.types, *t, &subst, self.resolved))
                    .collect();
                VariantKind::Tuple(pts)
            }
            VariantKind::Struct(fs) => {
                let fields: Vec<_> = fs
                    .iter()
                    .map(|(n, t)| {
                        (
                            *n,
                            Substitution::apply(&mut self.types, *t, &subst, self.resolved),
                        )
                    })
                    .collect();
                VariantKind::Struct(fields)
            }
        })
    }

    pub(in crate::typeck::check) fn generic_param_defs_for_fn(
        &self,
        f: &phx_syntax::ast::decl::Function,
        fn_def: DefId,
    ) -> Vec<DefId> {
        let Some(params) = f.generics.as_ref() else {
            return Vec::new();
        };
        let module = self
            .resolved
            .defs
            .get(fn_def.index() as usize)
            .map_or(self.current_module, |d| d.module);
        self.generic_param_defs_from_ast(module, params)
    }

    pub(in crate::typeck::check) fn generic_param_defs_from_ast(
        &self,
        module: u32,
        params: &[phx_syntax::ast::types::GenericParam],
    ) -> Vec<DefId> {
        params
            .iter()
            .filter_map(|param| {
                self.resolved
                    .resolutions
                    .get(&crate::resolver::ResolutionKey {
                        module,
                        node_id: param.name.id,
                    })
                    .copied()
                    .or_else(|| self.find_def(module, param.name.symbol, DefKind::GenericParam))
            })
            .collect()
    }

    pub(in crate::typeck::check) fn generic_param_bounds_for_def(
        &self,
        param_def: DefId,
    ) -> Option<Vec<Node<Type>>> {
        let record = self.resolved.defs.get(param_def.index() as usize)?;
        let module = record.module;
        let name = record.name;
        let param_name = self.resolved.interner.resolve(name)?;

        let impl_type = self
            .active_trait_impl
            .map(|(type_def, _)| type_def)
            .or_else(|| {
                self.mono_template_def
                    .and_then(|fn_def| self.impl_type_for_method(fn_def))
            });
        if let Some(impl_type) = impl_type {
            if let Some(impl_generics) = self.find_trait_impl_generics(impl_type) {
                for gp in &impl_generics {
                    if self
                        .resolved
                        .interner
                        .resolve(gp.name.symbol)
                        .is_some_and(|n| n == param_name)
                    {
                        return gp.bounds.clone();
                    }
                }
            }
        }

        for mod_item in &self.resolved.modules {
            if mod_item.id != module {
                continue;
            }
            for item in &mod_item.program.items {
                if let Some(bounds) = generic_bounds_in_decl(&item.inner.decl, name) {
                    return Some(bounds);
                }
            }
        }
        None
    }

    pub(in crate::typeck::check) fn find_trait_method_for_bounded_generic_param(
        &self,
        param_def: DefId,
        method: Symbol,
    ) -> Option<DefId> {
        let bounds = self.generic_param_bounds_for_def(param_def)?;
        for bound in bounds {
            let Some((trait_symbol, _)) = trait_bound_head(&bound.inner) else {
                continue;
            };
            let trait_def = self.type_defs.get(&trait_symbol).copied()?;
            let items = self.find_trait_items(trait_def)?;
            for item in items {
                if let TraitItem::Method(sig) = item {
                    let Some(method_name) = self.resolved.interner.resolve(method) else {
                        continue;
                    };
                    if self
                        .resolved
                        .interner
                        .resolve(sig.name.symbol)
                        .is_some_and(|name| name == method_name)
                    {
                        let trait_module = self.resolved.defs[trait_def.index() as usize].module;
                        return self.resolved.defs.iter().enumerate().find_map(|(i, d)| {
                            if d.module != trait_module || d.kind != DefKind::Fn {
                                return None;
                            }
                            if self
                                .resolved
                                .interner
                                .resolve(d.name)
                                .is_none_or(|n| n != method_name)
                            {
                                return None;
                            }
                            u32::try_from(i).ok().map(DefId::from_raw)
                        });
                    }
                }
            }
        }
        None
    }

    pub(in crate::typeck::check) fn find_trait_items(
        &self,
        trait_def: DefId,
    ) -> Option<&[TraitItem]> {
        let def = self.resolved.defs.get(trait_def.index() as usize)?;
        for module in &self.resolved.modules {
            if module.id != def.module {
                continue;
            }
            for item in &module.program.items {
                if let TopLevelDecl::Trait { name, items, .. } = &item.inner.decl {
                    if let Some(found) = self.find_def(module.id, name.symbol, DefKind::Trait) {
                        if found == trait_def {
                            return Some(items.as_slice());
                        }
                    }
                }
            }
        }
        None
    }

    pub(in crate::typeck::check) fn find_trait_generics(
        &self,
        trait_def: DefId,
    ) -> Option<Vec<phx_syntax::ast::types::GenericParam>> {
        let def = self.resolved.defs.get(trait_def.index() as usize)?;
        for module in &self.resolved.modules {
            if module.id != def.module {
                continue;
            }
            for item in &module.program.items {
                if let TopLevelDecl::Trait { name, generics, .. } = &item.inner.decl {
                    if let Some(found) = self.find_def(module.id, name.symbol, DefKind::Trait) {
                        if found == trait_def {
                            return generics.clone();
                        }
                    }
                }
            }
        }
        None
    }

    pub(in crate::typeck::check) fn build_trait_inst_key(
        &mut self,
        type_def: DefId,
        implementer_args: Vec<TypeId>,
        trait_ty: &Type,
        td: &TypeDefMap,
    ) -> Option<TraitInstKey> {
        let (trait_symbol, trait_arg_nodes) = trait_bound_head(trait_ty)?;
        let trait_def = self
            .trait_def_for_type(trait_ty)
            .or_else(|| self.type_defs.get(&trait_symbol).copied())?;
        let trait_args = trait_arg_nodes
            .map(|nodes| {
                nodes
                    .iter()
                    .map(|n| self.lower_ast_type_with_defs(n, td))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(TraitInstKey::new(
            type_def,
            implementer_args,
            trait_def,
            trait_args,
        ))
    }

    pub(in crate::typeck::check) fn trait_def_for_type(&self, trait_ty: &Type) -> Option<DefId> {
        let Type::Named { name, .. } = trait_ty else {
            return None;
        };
        let def = self.lookup_resolution(name.id)?;
        self.resolved
            .defs
            .get(def.index() as usize)
            .filter(|d| d.kind == DefKind::Trait)
            .map(|_| def)
    }

    pub(in crate::typeck::check) fn check_trait_impl_exhaustiveness(
        &mut self,
        type_name: &TypeName,
        trait_ty: &Node<Type>,
        members: &[ImplMember],
        span: Span,
    ) {
        let Some((trait_symbol, _)) = trait_bound_head(&trait_ty.inner) else {
            return;
        };
        let Some(trait_def) = self.type_defs.get(&trait_symbol).copied() else {
            return;
        };
        let Some(trait_items) = self.find_trait_items(trait_def).map(<[TraitItem]>::to_vec) else {
            return;
        };
        let impl_methods: std::collections::HashSet<Symbol> = members
            .iter()
            .filter_map(|m| match m {
                ImplMember::Method(f) => Some(f.name.symbol),
                ImplMember::AssociatedType { .. } => None,
            })
            .collect();
        let impl_assoc: std::collections::HashSet<Symbol> = members
            .iter()
            .filter_map(|m| match m {
                ImplMember::AssociatedType { name, .. } => Some(name.symbol),
                ImplMember::Method(_) => None,
            })
            .collect();
        let impl_has_method = |required: Symbol| {
            let Some(required_name) = self.resolved.interner.resolve(required) else {
                return false;
            };
            impl_methods.iter().any(|method| {
                self.resolved
                    .interner
                    .resolve(*method)
                    .is_some_and(|name| name == required_name)
            })
        };
        let impl_has_assoc = |required: Symbol| {
            let Some(required_name) = self.resolved.interner.resolve(required) else {
                return false;
            };
            impl_assoc.iter().any(|assoc| {
                self.resolved
                    .interner
                    .resolve(*assoc)
                    .is_some_and(|name| name == required_name)
            })
        };
        let type_display = self.symbol_name(type_name.symbol);
        let trait_display = self.symbol_name(trait_symbol);
        for item in trait_items {
            match item {
                TraitItem::AssociatedType(name) if !impl_has_assoc(name.symbol) => {
                    let assoc_display = self.symbol_name(name.symbol);
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::MissingAssociatedType {
                            type_name: type_display.clone(),
                            trait_name: trait_display.clone(),
                            assoc_name: assoc_display,
                            span,
                        },
                    );
                }
                TraitItem::AssociatedType(_) => {}
                TraitItem::Method(sig) => {
                    if sig.body.is_some() || impl_has_method(sig.name.symbol) {
                        continue;
                    }
                    let method_display = self.symbol_name(sig.name.symbol);
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::MissingTraitMethod {
                            type_name: type_display.clone(),
                            trait_name: trait_display.clone(),
                            method_name: method_display,
                            span,
                        },
                    );
                }
            }
        }
    }

    pub(in crate::typeck::check) fn find_inherent_impl_generics(
        &self,
        type_def: DefId,
    ) -> Option<Vec<phx_syntax::ast::types::GenericParam>> {
        let def = self.resolved.defs.get(type_def.index() as usize)?;
        for module in &self.resolved.modules {
            if module.id != def.module {
                continue;
            }
            for item in &module.program.items {
                if let TopLevelDecl::Impl {
                    type_name,
                    generics,
                    trait_,
                    ..
                } = &item.inner.decl
                {
                    if trait_.is_some() {
                        continue;
                    }
                    if let Some(struct_def) = self
                        .find_def(module.id, type_name.symbol, DefKind::Struct)
                        .or_else(|| self.find_def(module.id, type_name.symbol, DefKind::Enum))
                    {
                        if struct_def == type_def {
                            return generics.clone();
                        }
                    }
                }
            }
        }
        None
    }

    pub(in crate::typeck::check) fn find_trait_impl_generics(
        &self,
        type_def: DefId,
    ) -> Option<Vec<phx_syntax::ast::types::GenericParam>> {
        let def = self.resolved.defs.get(type_def.index() as usize)?;
        for module in &self.resolved.modules {
            if module.id != def.module {
                continue;
            }
            for item in &module.program.items {
                if let TopLevelDecl::Impl {
                    type_name,
                    generics,
                    trait_,
                    ..
                } = &item.inner.decl
                {
                    if trait_.is_none() {
                        continue;
                    }
                    if let Some(struct_def) = self
                        .find_def(module.id, type_name.symbol, DefKind::Struct)
                        .or_else(|| self.find_def(module.id, type_name.symbol, DefKind::Enum))
                    {
                        if struct_def == type_def {
                            return generics.clone();
                        }
                    }
                }
            }
        }
        None
    }

    /// Generic parameters for checking or monomorphizing methods on `type_def`.
    pub(in crate::typeck::check) fn generic_params_for_impl_type(
        &self,
        type_def: DefId,
    ) -> Option<Vec<phx_syntax::ast::types::GenericParam>> {
        self.find_inherent_impl_generics(type_def)
            .or_else(|| self.find_trait_impl_generics(type_def))
            .or_else(|| generic_params_for_def(self.resolved, type_def))
    }

    pub(in crate::typeck::check) fn impl_generic_param_defs(&self, type_def: DefId) -> Vec<DefId> {
        let module = self
            .resolved
            .defs
            .get(type_def.index() as usize)
            .map_or(self.current_module, |d| d.module);
        if let Some(params) = self.generic_params_for_impl_type(type_def) {
            return self.generic_param_defs_from_ast(module, &params);
        }
        generic_param_defs_for_type(self.resolved, type_def).unwrap_or_default()
    }

    pub(in crate::typeck::check) fn receiver_type_args(
        &self,
        type_def: DefId,
        receiver: TypeId,
    ) -> Option<Vec<TypeId>> {
        let (def, args) = self.named_type_under_receiver(receiver)?;
        if def != type_def {
            return None;
        }
        Some(args)
    }

    pub(in crate::typeck::check) fn method_receiver_matches(
        &self,
        param: TypeId,
        receiver: TypeId,
    ) -> bool {
        if self.types_equal(receiver, param) {
            return true;
        }
        if let Ty::Ref { inner, .. } = self.types.get(param) {
            return self.types_equal(receiver, *inner);
        }
        false
    }

    pub(in crate::typeck::check) fn method_arg_param_types(
        &self,
        f: &Function,
        params: &[TypeId],
        receiver: TypeId,
    ) -> Vec<TypeId> {
        let has_receiver = f.params.iter().any(|p| matches!(p, Param::Receiver { .. }));
        if has_receiver && !params.is_empty() {
            return params[1..].to_vec();
        }
        if !params.is_empty() && self.method_receiver_matches(params[0], receiver) {
            return params[1..].to_vec();
        }
        params.to_vec()
    }

    pub(in crate::typeck::check) fn find_function_decl(&self, def: DefId) -> Option<&Function> {
        if let Some(f) = self.inherited_trait_methods.get(&def) {
            return Some(f);
        }
        for module in &self.resolved.modules {
            for item in &module.program.items {
                match &item.inner.decl {
                    TopLevelDecl::Function(f)
                        if self.find_def(module.id, f.name.symbol, DefKind::Fn) == Some(def) =>
                    {
                        return Some(f);
                    }
                    TopLevelDecl::Impl { members, .. } => {
                        for m in members {
                            if let ImplMember::Method(f) = m {
                                if self.find_def(module.id, f.name.symbol, DefKind::Fn) == Some(def)
                                {
                                    return Some(f);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    pub(in crate::typeck::check) fn context_generic_args_for_enum(
        &self,
        enum_def: DefId,
    ) -> Option<Vec<TypeId>> {
        for ctx in [self.ctor_expected, self.fn_ret] {
            let Some(ty) = ctx else {
                continue;
            };
            if let Ty::Named { def, args } = self.types.get(ty).clone() {
                if def == enum_def && !args.is_empty() {
                    return Some(args);
                }
            }
        }
        None
    }

    pub(in crate::typeck::check) fn check_try_expr(
        &mut self,
        scrutinee_ty: TypeId,
        span: Span,
        expr_id: ExprId,
    ) -> TypeId {
        let Some(fn_ret) = self.fn_ret else {
            self.bag.push(
                self.current_module,
                TypeCheckError::TryOutsideFunction { span },
            );
            return self.unit;
        };
        if self.std_kernel.is_std_result(&self.types, scrutinee_ty)
            && self.std_kernel.is_std_result(&self.types, fn_ret)
        {
            return self.check_result_try_expr(scrutinee_ty, fn_ret, span, expr_id);
        }
        if self.std_kernel.is_std_option(&self.types, scrutinee_ty)
            && self.std_kernel.is_std_option(&self.types, fn_ret)
            && self.types_equal(scrutinee_ty, fn_ret)
        {
            let Some(payload) = self.std_kernel.option_payload_ty(&self.types, scrutinee_ty) else {
                return self.unit;
            };
            self.record_try_site(
                expr_id,
                scrutinee_ty,
                payload,
                TryFailureMode::ReturnScrutinee,
                span,
            );
            return payload;
        }
        self.bag.push(
            self.current_module,
            TypeCheckError::InvalidTryOperand {
                found: self.format_ty(scrutinee_ty),
                expected_return: self.format_ty(fn_ret),
                span,
            },
        );
        self.unit
    }

    pub(in crate::typeck::check) fn check_result_try_expr(
        &mut self,
        scrutinee_ty: TypeId,
        fn_ret: TypeId,
        span: Span,
        expr_id: ExprId,
    ) -> TypeId {
        let Some((ok_in, err_in)) = self.std_kernel.result_ok_err_tys(&self.types, scrutinee_ty)
        else {
            return self.unit;
        };
        let Some((ok_out, err_out)) = self.std_kernel.result_ok_err_tys(&self.types, fn_ret) else {
            return self.unit;
        };
        if !self.types_equal(ok_in, ok_out) {
            self.bag.push(
                self.current_module,
                TypeCheckError::InvalidTryOperand {
                    found: self.format_ty(scrutinee_ty),
                    expected_return: self.format_ty(fn_ret),
                    span,
                },
            );
            return self.unit;
        }
        if self.types_equal(err_in, err_out) {
            self.record_try_site(
                expr_id,
                scrutinee_ty,
                ok_in,
                TryFailureMode::ReturnScrutinee,
                span,
            );
            return ok_in;
        }
        if let Some(from_fn) = resolve_from_fn_for_error(
            &self.program_layout,
            &self.types,
            &self.std_trait_kernel,
            self.resolved,
            err_out,
            err_in,
        ) {
            if let Some(f) = self.find_function_decl(from_fn).cloned() {
                let param_defs = self.generic_param_defs_for_fn(&f, from_fn);
                if !param_defs.is_empty() {
                    self.record_mono_inst(
                        from_fn,
                        vec![err_in],
                        phx_syntax::AstNodeId::synthetic(expr_id.index()),
                    );
                }
            }
            self.record_try_site(
                expr_id,
                scrutinee_ty,
                ok_in,
                TryFailureMode::ConvertErr {
                    err_in_ty: err_in,
                    err_out_ty: err_out,
                    return_result_ty: fn_ret,
                    from_fn,
                },
                span,
            );
            return ok_in;
        }
        self.bag.push(
            self.current_module,
            TypeCheckError::TryErrorFromMissing {
                err_in: self.format_ty(err_in),
                err_out: self.format_ty(err_out),
                span,
            },
        );
        self.unit
    }

    pub(in crate::typeck::check) fn record_try_site(
        &mut self,
        expr_id: ExprId,
        scrutinee_ty: TypeId,
        success_ty: TypeId,
        failure_mode: TryFailureMode,
        declare_span: Span,
    ) {
        let Ty::Named { def, args } = self.types.get(scrutinee_ty).clone() else {
            return;
        };
        let Some(success_tag) =
            self.std_kernel
                .success_tag_for(&self.program_layout, &self.types, scrutinee_ty)
        else {
            return;
        };
        let Some(failure_tag) =
            self.std_kernel
                .failure_tag_for(&self.program_layout, &self.types, scrutinee_ty)
        else {
            return;
        };
        let Some(layout) = &mut self.layout else {
            return;
        };
        let temp_slot = layout.alloc_match_scrutinee_temp(scrutinee_ty, declare_span);
        self.try_sites.insert(
            expr_id,
            TrySiteMeta {
                enum_def: def,
                enum_args: args,
                scrutinee_ty,
                success_ty,
                success_tag,
                failure_tag,
                temp_slot,
                failure_mode,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::typeck::check) fn resolve_concrete_generic_args(
        &mut self,
        generic_nodes: Option<&[Node<Type>]>,
        fn_generics: Option<&[phx_syntax::ast::types::GenericParam]>,
        param_defs: &[DefId],
        param_types: &[TypeId],
        args: &[ExprNode],
        span: Span,
        enum_def: Option<DefId>,
        fn_module: u32,
    ) -> Option<Vec<TypeId>> {
        if let Some(nodes) = generic_nodes {
            if nodes.len() > param_defs.len() {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::ArityMismatch {
                        expected: param_defs.len(),
                        found: nodes.len(),
                        span,
                    },
                );
                return None;
            }
            if nodes.len() < param_defs.len() {
                return self.complete_generic_args_from_ast(
                    fn_generics,
                    param_defs,
                    nodes,
                    fn_module,
                    span,
                );
            }
            let type_defs = self.type_defs.clone();
            let mut concrete_args = Vec::new();
            for ty_node in nodes {
                concrete_args.push(self.lower_ast_type_with_defs(ty_node, &type_defs));
            }
            return Some(concrete_args);
        }
        let mut infer = InferenceCtx::new();
        let mut subst = Substitution::new();
        for param_def in param_defs {
            let var = infer.fresh_var(&mut self.types);
            subst.insert(*param_def, var);
        }
        let applied: Vec<_> = param_types
            .iter()
            .map(|p| Substitution::apply(&mut self.types, *p, &subst, self.resolved))
            .collect();
        let mut arg_types = Vec::with_capacity(args.len());
        for arg in args {
            arg_types.push(self.check_expr_node_infer(arg));
        }
        let defs = &self.resolved.defs;
        let value_types = &self.value_types;
        for (p, got) in applied.iter().zip(&arg_types) {
            if !infer.unify(&mut self.types, defs, value_types, *p, *got) {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::InferenceAmbiguous { span },
                );
                return None;
            }
        }
        let mut concrete_args = Vec::new();
        for param_def in param_defs {
            let var = subst.get(*param_def)?;
            let resolved = infer.resolve(&mut self.types, var);
            if !infer.is_resolved(&mut self.types, resolved) {
                if let Some(enum_def) = enum_def {
                    if let Some(ctx) = self.context_generic_args_for_enum(enum_def) {
                        if ctx.len() == param_defs.len() {
                            return Some(ctx);
                        }
                    }
                }
                self.bag.push(
                    self.current_module,
                    TypeCheckError::InferenceFailed { span },
                );
                return None;
            }
            concrete_args.push(resolved);
        }
        Some(concrete_args)
    }

    /// Fills trailing generic parameters from call arguments before applying defaults.
    pub(in crate::typeck::check) fn complete_partial_generic_args_with_inference(
        &mut self,
        type_def: DefId,
        param_defs: &[DefId],
        mut provided: Vec<TypeId>,
        param_types: &[TypeId],
        args: &[ExprNode],
        span: Span,
    ) -> Option<Vec<TypeId>> {
        if provided.len() >= param_defs.len() {
            return Some(provided);
        }
        let generic_params = generic_params_for_def(self.resolved, type_def)?;
        let mut subst = Substitution::new();
        for (i, param_def) in param_defs.iter().enumerate() {
            if i < provided.len() {
                subst.insert(*param_def, provided[i]);
            }
        }
        let mut infer = InferenceCtx::new();
        for param_def in &param_defs[provided.len()..] {
            let var = infer.fresh_var(&mut self.types);
            subst.insert(*param_def, var);
        }
        let applied: Vec<_> = param_types
            .iter()
            .map(|p| Substitution::apply(&mut self.types, *p, &subst, self.resolved))
            .collect();
        let mut arg_types = Vec::with_capacity(args.len());
        for arg in args {
            arg_types.push(self.check_expr_node_infer(arg));
        }
        let defs = &self.resolved.defs;
        let value_types = &self.value_types;
        let mut inferred_ok = true;
        for (p, got) in applied.iter().zip(&arg_types) {
            if !infer.unify(&mut self.types, defs, value_types, *p, *got) {
                inferred_ok = false;
                break;
            }
        }
        if inferred_ok {
            let start = provided.len();
            for param_def in param_defs.iter().skip(start) {
                let var = subst.get(*param_def)?;
                let resolved = infer.resolve(&mut self.types, var);
                if !infer.is_resolved(&mut self.types, resolved) {
                    inferred_ok = false;
                    break;
                }
                provided.push(resolved);
            }
        }
        let _ = inferred_ok;
        if provided.len() < param_defs.len() {
            let type_defs = self.type_defs.clone();
            let module = self.def_module(type_def);
            for (i, &param_def) in param_defs.iter().enumerate().skip(provided.len()) {
                let param = generic_params.get(i)?;
                let default = param.default.as_ref()?;
                let lowered = self.with_pushed_generics(module, Some(&generic_params), |this| {
                    this.lower_ast_type_with_defs(default, &type_defs)
                });
                let concrete = Substitution::apply(&mut self.types, lowered, &subst, self.resolved);
                subst.insert(param_def, concrete);
                provided.push(concrete);
            }
        }
        if provided.len() == param_defs.len() {
            Some(provided)
        } else {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: param_defs.len(),
                    found: provided.len(),
                    span,
                },
            );
            None
        }
    }

    pub(in crate::typeck::check) fn check_call_with_generics(
        &mut self,
        callee: TypeId,
        callee_def: Option<DefId>,
        base: &ExprNode,
        generics: Option<&[Node<Type>]>,
        args: &[ExprNode],
        span: Span,
    ) -> TypeId {
        let Some(fn_def) = callee_def else {
            return self.check_call(callee, args, span);
        };
        self.check_unsafe_fn_call(fn_def, span);
        let Some(f) = self.find_function_decl(fn_def).cloned() else {
            if self.program_layout.tuple_structs.contains(&fn_def) {
                return self.check_tuple_struct_ctor_call(fn_def, generics, args, span);
            }
            if let Some(enum_def) = self.enum_def_for_variant(fn_def) {
                return self.check_enum_variant_call_with_generics(
                    enum_def, fn_def, callee, generics, args, span,
                );
            }
            return self.check_call(callee, args, span);
        };
        let param_defs = self.generic_param_defs_for_fn(&f, fn_def);
        let fn_generics = f.generics.clone();
        if param_defs.is_empty() {
            if generics.is_some() {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::UnsupportedFeature {
                        feature: "type arguments on non-generic call",
                        span,
                    },
                );
            }
            return self.check_call(callee, args, span);
        }
        let Ty::Fn { params, ret } = self.types.get(callee).clone() else {
            return self.check_call(callee, args, span);
        };
        let Some(concrete_args) = self.resolve_concrete_generic_args(
            generics,
            fn_generics.as_deref(),
            &param_defs,
            &params,
            args,
            span,
            None,
            self.def_module(fn_def),
        ) else {
            return self.unit;
        };
        let mut subst = Substitution::new();
        for (param_def, concrete) in param_defs.iter().zip(&concrete_args) {
            subst.insert(*param_def, *concrete);
        }
        let params: Vec<_> = params
            .iter()
            .map(|p| Substitution::apply(&mut self.types, *p, &subst, self.resolved))
            .collect();
        let ret = Substitution::apply(&mut self.types, ret, &subst, self.resolved);
        if params.len() != args.len() {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: params.len(),
                    found: args.len(),
                    span,
                },
            );
        }
        for (index, (p, arg)) in params.iter().zip(args.iter()).enumerate() {
            let got = self.check_expr_node(arg);
            if !self.types_equal(got, *p) {
                self.error_mismatch(*p, got, arg.span, MismatchKind::Argument { index });
            }
        }
        let site = callee_name_use_id(base);
        if let Some(site) = site {
            self.record_mono_inst(fn_def, concrete_args, site);
        }
        ret
    }

    pub(in crate::typeck::check) fn check_enum_variant_call_with_generics(
        &mut self,
        enum_def: DefId,
        variant_def: DefId,
        callee: TypeId,
        generics: Option<&[Node<Type>]>,
        args: &[ExprNode],
        span: Span,
    ) -> TypeId {
        let param_defs = generic_param_defs_for_type(self.resolved, enum_def).unwrap_or_default();
        if param_defs.is_empty() {
            if generics.is_some() {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::UnsupportedFeature {
                        feature: "type arguments on non-generic enum constructor",
                        span,
                    },
                );
            }
            return self.check_call(callee, args, span);
        }
        let Ty::Fn {
            params: ctor_params,
            ..
        } = self.types.get(callee).clone()
        else {
            return self.check_call(callee, args, span);
        };
        let concrete_args = if generics.is_none() {
            self.context_generic_args_for_enum(enum_def)
        } else {
            None
        };
        let Some(concrete_args) = concrete_args.or_else(|| {
            self.resolve_concrete_generic_args(
                generics,
                generic_params_for_def(self.resolved, enum_def).as_deref(),
                &param_defs,
                &ctor_params,
                args,
                span,
                Some(enum_def),
                self.def_module(enum_def),
            )
        }) else {
            return self.unit;
        };
        self.record_type_mono_inst(enum_def, TypeMonoKind::Enum, concrete_args.clone());
        let enum_ty = self.types.intern(&Ty::Named {
            def: enum_def,
            args: concrete_args.clone(),
        });
        let Some(payload) = self.substituted_variant_payload(variant_def, &concrete_args) else {
            return self.check_call(callee, args, span);
        };
        let params = match payload {
            VariantKind::Unit => vec![],
            VariantKind::Tuple(ts) => ts,
            VariantKind::Struct(fs) => fs.into_iter().map(|(_, t)| t).collect(),
        };
        if params.len() != args.len() {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: params.len(),
                    found: args.len(),
                    span,
                },
            );
        }
        for (index, (p, arg)) in params.iter().zip(args.iter()).enumerate() {
            let got = self.check_expr_node(arg);
            if !self.types_equal(got, *p) {
                self.error_mismatch(*p, got, arg.span, MismatchKind::Argument { index });
            }
        }
        enum_ty
    }

    pub(in crate::typeck::check) fn check_call(
        &mut self,
        callee: TypeId,
        args: &[ExprNode],
        span: Span,
    ) -> TypeId {
        let Ty::Fn { params, ret } = self.types.get(callee).clone() else {
            self.bag.push(
                self.current_module,
                TypeCheckError::NotCallable {
                    found: self.format_ty(callee),
                    span,
                },
            );
            return self.unit;
        };
        {
            if params.len() != args.len() {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::ArityMismatch {
                        expected: params.len(),
                        found: args.len(),
                        span,
                    },
                );
            }
            for (index, (p, arg)) in params.iter().zip(args.iter()).enumerate() {
                let got = self.check_expr_node(arg);
                if !self.types_equal(got, *p) {
                    self.error_mismatch(*p, got, arg.span, MismatchKind::Argument { index });
                }
            }
            ret
        }
    }

    pub(in crate::typeck::check) fn check_index(&mut self, base: TypeId, span: Span) -> TypeId {
        match self.types.get(base) {
            Ty::Array { elem, .. } | Ty::Slice(elem) => *elem,
            Ty::Tuple(elems) if !elems.is_empty() => elems[0],
            _ => {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::InvalidOperator { op: "index", span },
                );
                self.unit
            }
        }
    }

    pub(in crate::typeck::check) fn named_type_under_receiver(
        &self,
        receiver: TypeId,
    ) -> Option<(DefId, Vec<TypeId>)> {
        let named = match self.types.get(receiver).clone() {
            Ty::Named { .. } => receiver,
            Ty::Ref { inner, .. } => inner,
            _ => return None,
        };
        match self.types.get(named).clone() {
            Ty::Named { def, args } => Some((def, args)),
            _ => None,
        }
    }

    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub(in crate::typeck::check) fn check_method_call_with_generics(
        &mut self,
        receiver_expr: Option<&ExprNode>,
        receiver: TypeId,
        name: &Ident,
        generics: Option<&[Node<Type>]>,
        args: &[ExprNode],
        span: Span,
        site_id: ExprId,
    ) -> TypeId {
        if let Ty::Primitive(kw) = self.types.get(receiver).clone() {
            return self.check_primitive_method_call(kw, receiver, name, args, span, site_id);
        }
        let Some((type_def, implementer_args)) = self.named_type_under_receiver(receiver) else {
            return self.emit_unresolved_method(receiver, name, span);
        };
        let fn_def = self
            .program_layout
            .inherent_methods
            .get(&(type_def, name.symbol))
            .copied()
            .or_else(|| {
                find_trait_method_def(
                    &self.program_layout,
                    type_def,
                    &implementer_args,
                    name.symbol,
                )
            })
            .or_else(|| {
                if self
                    .resolved
                    .defs
                    .get(type_def.index() as usize)
                    .is_some_and(|d| d.kind == DefKind::GenericParam)
                {
                    self.find_trait_method_for_bounded_generic_param(type_def, name.symbol)
                } else {
                    None
                }
            });
        let Some(fn_def) = fn_def else {
            return self.emit_ambiguous_or_unresolved_method(receiver, type_def, name, span);
        };
        self.check_unsafe_fn_call(fn_def, span);
        let Some(&fn_ty) = self.value_types.get(&fn_def) else {
            return self.emit_unresolved_method(receiver, name, span);
        };
        let Some(f) = self.find_function_decl(fn_def).cloned() else {
            return self.check_call(fn_ty, args, span);
        };
        let impl_param_defs = self.impl_generic_param_defs(type_def);
        let method_param_defs = self.generic_param_defs_for_fn(&f, fn_def);
        let Ty::Fn { params, ret } = self.types.get(fn_ty).clone() else {
            return self.unit;
        };
        let arg_param_types = self.method_arg_param_types(&f, &params, receiver);
        let impl_args = self
            .receiver_type_args(type_def, receiver)
            .unwrap_or_default();
        if impl_param_defs.len() != impl_args.len() && !impl_param_defs.is_empty() {
            self.bag.push(
                self.current_module,
                TypeCheckError::Mismatch {
                    expected: "specialized receiver type".to_owned(),
                    found: self.format_ty(receiver),
                    span,
                    kind: MismatchKind::default(),
                },
            );
            return self.unit;
        }
        let needs_mono = !impl_param_defs.is_empty() || !method_param_defs.is_empty();
        if needs_mono {
            let mut subst = Substitution::new();
            for (param_def, concrete) in impl_param_defs.iter().zip(&impl_args) {
                subst.insert(*param_def, *concrete);
            }
            let infer_arg_params: Vec<_> = arg_param_types
                .iter()
                .map(|p| Substitution::apply(&mut self.types, *p, &subst, self.resolved))
                .collect();
            let method_args = if method_param_defs.is_empty() {
                Vec::new()
            } else {
                let Some(concrete) = self.resolve_concrete_generic_args(
                    generics,
                    f.generics.as_deref(),
                    &method_param_defs,
                    &infer_arg_params,
                    args,
                    span,
                    None,
                    self.def_module(fn_def),
                ) else {
                    return self.unit;
                };
                concrete
            };
            if generics.is_some() && method_param_defs.is_empty() {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::UnsupportedFeature {
                        feature: "type arguments on non-generic method",
                        span,
                    },
                );
            }
            for (param_def, concrete) in method_param_defs.iter().zip(&method_args) {
                subst.insert(*param_def, *concrete);
            }
            let applied_arg_params: Vec<_> = arg_param_types
                .iter()
                .map(|p| Substitution::apply(&mut self.types, *p, &subst, self.resolved))
                .collect();
            if applied_arg_params.len() != args.len() {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::ArityMismatch {
                        expected: applied_arg_params.len(),
                        found: args.len(),
                        span,
                    },
                );
            }
            for (index, (p, arg)) in applied_arg_params.iter().zip(args).enumerate() {
                let got = self.check_expr_node(arg);
                if !self.types_equal(got, *p) {
                    self.error_mismatch(*p, got, arg.span, MismatchKind::Argument { index });
                }
            }
            let mut mono_args = impl_args;
            mono_args.extend(method_args);
            self.record_mono_inst(fn_def, mono_args.clone(), name.id);
            self.method_call_sites.insert(
                site_id,
                MethodCallSiteMeta {
                    template: fn_def,
                    mono_args,
                },
            );
            let out = Substitution::apply(&mut self.types, ret, &subst, self.resolved);
            if let Some(expr) = receiver_expr {
                self.mark_method_receiver_moved(expr, receiver, fn_def);
            }
            return out;
        }
        if generics.is_some() {
            self.bag.push(
                self.current_module,
                TypeCheckError::UnsupportedFeature {
                    feature: "type arguments on non-generic method",
                    span,
                },
            );
        }
        if arg_param_types.len() != args.len() {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: arg_param_types.len(),
                    found: args.len(),
                    span,
                },
            );
        }
        for (index, (p, arg)) in arg_param_types.iter().zip(args).enumerate() {
            let got = self.check_expr_node(arg);
            if !self.types_equal(got, *p) {
                self.error_mismatch(*p, got, arg.span, MismatchKind::Argument { index });
            }
        }
        if let Some(expr) = receiver_expr {
            self.mark_method_receiver_moved(expr, receiver, fn_def);
        }
        self.method_call_sites.insert(
            site_id,
            MethodCallSiteMeta {
                template: fn_def,
                mono_args: Vec::new(),
            },
        );
        ret
    }

    pub(in crate::typeck::check) fn check_primitive_method_call(
        &mut self,
        kw: Keyword,
        receiver: TypeId,
        name: &Ident,
        args: &[ExprNode],
        span: Span,
        site_id: ExprId,
    ) -> TypeId {
        if self.resolved.interner.resolves_to(name.symbol, "eq") {
            if let Some(eq_trait) = self.std_trait_kernel.partial_eq_trait {
                if self.std_trait_kernel.primitive_satisfies(kw, eq_trait) {
                    if args.len() == 1 {
                        let got = self.check_expr_node(&args[0]);
                        if !self.types_equal(got, receiver) {
                            self.error_mismatch(
                                receiver,
                                got,
                                args[0].span,
                                MismatchKind::Argument { index: 0 },
                            );
                        }
                    } else {
                        self.bag.push(
                            self.current_module,
                            TypeCheckError::ArityMismatch {
                                expected: 1,
                                found: args.len(),
                                span,
                            },
                        );
                    }
                    self.primitive_method_sites
                        .insert(site_id, PrimitiveMethodSite::Eq);
                    return self.bool_ty;
                }
            }
        }
        if self.resolved.interner.resolves_to(name.symbol, "clone") {
            if let Some(clone_trait) = self.std_trait_kernel.clone_trait {
                if self.std_trait_kernel.primitive_satisfies(kw, clone_trait) {
                    if !args.is_empty() {
                        self.bag.push(
                            self.current_module,
                            TypeCheckError::ArityMismatch {
                                expected: 0,
                                found: args.len(),
                                span,
                            },
                        );
                    }
                    self.primitive_method_sites
                        .insert(site_id, PrimitiveMethodSite::Clone);
                    return receiver;
                }
            }
        }
        self.emit_unresolved_method(receiver, name, span)
    }

    pub(in crate::typeck::check) fn emit_unresolved_method(
        &mut self,
        receiver: TypeId,
        name: &Ident,
        span: Span,
    ) -> TypeId {
        self.bag.push(
            self.current_module,
            TypeCheckError::UnresolvedMethod {
                receiver: self.format_ty(receiver),
                method_index: name.symbol.index(),
                span,
            },
        );
        self.unit
    }

    pub(in crate::typeck::check) fn emit_ambiguous_or_unresolved_method(
        &mut self,
        receiver: TypeId,
        type_def: DefId,
        name: &Ident,
        span: Span,
    ) -> TypeId {
        let mut trait_matches: Vec<DefId> = self
            .program_layout
            .trait_methods
            .iter()
            .filter(|((key, method), _)| key.implementer == type_def && *method == name.symbol)
            .map(|(_, fn_def)| *fn_def)
            .collect();
        trait_matches.sort_by_key(|d| d.index());
        trait_matches.dedup();
        if trait_matches.len() > 1 {
            self.bag.push(
                self.current_module,
                TypeCheckError::AmbiguousMethod {
                    receiver: self.format_ty(receiver),
                    method_index: name.symbol.index(),
                    span,
                },
            );
        }
        self.emit_unresolved_method(receiver, name, span)
    }

    pub(in crate::typeck::check) fn check_if(
        &mut self,
        condition: &IfCondition,
        then_block: &BlockNode,
        else_ifs: &[(IfCondition, BlockNode)],
        else_block: &Option<BlockNode>,
        span: Span,
    ) -> TypeId {
        let pre = self.ownership.clone();
        let mut arm_states = Vec::new();
        let (mut then_ty, end) =
            self.check_with_ownership_fork(&pre, |this| this.check_if_arm(condition, then_block));
        arm_states.push(end);
        for (ec, eb) in else_ifs {
            let (arm_ty, end) =
                self.check_with_ownership_fork(&pre, |this| this.check_if_arm(ec, eb));
            arm_states.push(end);
            then_ty = unify_branch(&self.alias_env(), then_ty, arm_ty).unwrap_or_else(|| {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::NonUnifyingBranches { span },
                );
                self.unit
            });
        }
        if let Some(else_b) = else_block {
            let (arm_ty, end) =
                self.check_with_ownership_fork(&pre, |this| this.check_block_expr(else_b));
            arm_states.push(end);
            then_ty = unify_branch(&self.alias_env(), then_ty, arm_ty).unwrap_or_else(|| {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::NonUnifyingBranches { span },
                );
                self.unit
            });
        }
        self.ownership = OwnershipTracker::join_arms(&pre, &arm_states);
        then_ty
    }

    pub(in crate::typeck::check) fn check_if_arm(
        &mut self,
        condition: &IfCondition,
        then_block: &BlockNode,
    ) -> TypeId {
        match condition {
            IfCondition::Bool(cond) => {
                let c = self.check_expr_node(cond);
                if !self.types_equal(c, self.bool_ty) {
                    self.error_mismatch(self.bool_ty, c, cond.span, MismatchKind::Condition);
                }
                self.check_block_expr(then_block)
            }
            IfCondition::Pattern {
                mutable,
                pattern,
                scrutinee,
            } => {
                let s = self.check_expr_node(scrutinee);
                self.record_scrutinee_type_mono(s);
                if let Some(layout) = &mut self.layout {
                    let _ = layout.alloc_match_scrutinee_temp(s, scrutinee.span);
                }
                let kind = if *mutable {
                    BindingKind::Var
                } else {
                    BindingKind::Const
                };
                self.enter_scope();
                self.check_pattern(&pattern.inner, s, pattern.span, kind);
                let body_ty = self.check_block_expr(then_block);
                self.exit_scope();
                body_ty
            }
        }
    }

    pub(in crate::typeck::check) fn is_borrow_type(&self, ty: TypeId) -> bool {
        matches!(self.types.get(ty), Ty::Slice(_) | Ty::Str | Ty::Ref { .. })
    }

    pub(in crate::typeck::check) fn check_utf8_array_to_str_cast(
        &self,
        from: TypeId,
        to: TypeId,
        expr: &Expr,
    ) -> bool {
        if !matches!(self.types.get(to), Ty::Str) {
            return false;
        }
        if !matches!(
            self.types.get(from),
            Ty::Array {
                elem,
                ..
            } if matches!(self.types.get(*elem), Ty::Primitive(phx_syntax::token::Keyword::U8))
        ) {
            return false;
        }
        match expr {
            Expr::Literal(Literal::ByteString(b)) => std::str::from_utf8(b).is_ok(),
            Expr::Ident(ident) => self.layout.as_ref().is_some_and(|layout| {
                layout
                    .binding(ident.symbol)
                    .is_some_and(|b| b.kind == BindingKind::Const && b.utf8_rodata.is_some())
            }),
            _ => false,
        }
    }

    pub(in crate::typeck::check) fn binding_kind_for_ident(
        &self,
        ident: Ident,
    ) -> Option<BindingKind> {
        self.layout.as_ref()?.binding(ident.symbol).map(|b| b.kind)
    }

    pub(in crate::typeck::check) fn local_binding_escapes(kind: BindingKind) -> bool {
        matches!(
            kind,
            BindingKind::Var | BindingKind::Const | BindingKind::MatchTemp
        )
    }

    pub(in crate::typeck::check) fn expr_borrow_site(&mut self, expr: &Expr) -> Option<Span> {
        match expr {
            Expr::Ident(ident) => {
                let kind = self.binding_kind_for_ident(*ident)?;
                if Self::local_binding_escapes(kind) {
                    Some(ident.span)
                } else {
                    None
                }
            }
            Expr::Unary {
                op: UnaryOp::Ref | UnaryOp::RefMut,
                operand,
            } => self.expr_borrow_site(&operand.inner),
            Expr::Unary {
                op: UnaryOp::Deref,
                operand,
            } => self.expr_borrow_site(&operand.inner),
            Expr::Cast { expr, ty } => {
                let to = self.lower_ast_type(ty);
                if matches!(self.types.get(to), Ty::Slice(_) | Ty::Str) {
                    self.expr_borrow_site(&expr.inner)
                } else {
                    None
                }
            }
            Expr::Postfix { base, .. } => self.expr_borrow_site(&base.inner),
            _ => None,
        }
    }

    pub(in crate::typeck::check) fn check_expr_escapes_local(&mut self, expr: &ExprNode) {
        if let Some(borrow_span) = self.expr_borrow_site(&expr.inner) {
            self.bag.push(
                self.current_module,
                TypeCheckError::ReturnEscapesLocal {
                    span: expr.span,
                    borrow_span,
                },
            );
        }
    }
}

fn callee_name_use_id(base: &ExprNode) -> Option<phx_syntax::AstNodeId> {
    match &base.inner {
        Expr::Ident(ident) => Some(ident.id),
        Expr::Path(path) if path.segments.len() == 1 => match &path.segments[0] {
            PathSegment::Ident(ident) => Some(ident.id),
            PathSegment::Type(seg) => Some(seg.name.id),
        },
        _ => None,
    }
}

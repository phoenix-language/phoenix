//! Inherent and trait impl checking.

use phx_diagnostics::{MismatchKind, Span, TypeCheckError};
use phx_syntax::ast::decl::{Function, ImplMember, Param, TopLevelDecl, TraitItem};
use phx_syntax::ast::expr::{Expr, IfCondition, LambdaBody, PostfixOp, StructFieldInit};
use phx_syntax::ast::ident::TypeName;
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};
use phx_syntax::ast::types::Type;
use phx_syntax::ast::{ExprNode, Node};
use phx_syntax::{Symbol, impl_receiver_symbol};

use super::TypeChecker;
use crate::resolver::{DefId, DefKind, ResolvedProgram};
use crate::typeck::bindings::BindingKind;
use crate::typeck::bounds::trait_bound_head;
use crate::typeck::builtins::implements_drop_for_def;
use crate::typeck::layout::ProgramLayout;
use crate::typeck::lower_ty::{TypeDefMap, push_generics};
use crate::typeck::subst::Substitution;
use crate::typeck::types::{Ty, TypeId};

impl TypeChecker<'_> {
    pub(in crate::typeck::check) fn trait_method_sig_unsafe(
        sig: &phx_syntax::ast::decl::FunctionSig,
        trait_unsafe: bool,
    ) -> bool {
        trait_unsafe || sig.unsafe_
    }

    pub(in crate::typeck::check) fn validate_impl_unsafe(
        &mut self,
        type_name: &TypeName,
        trait_ty: &Node<Type>,
        impl_unsafe: bool,
        members: &[ImplMember],
        span: Span,
    ) {
        let Some((trait_symbol, _)) = trait_bound_head(&trait_ty.inner) else {
            return;
        };
        let Some(&trait_def) = self.type_defs.get(&trait_symbol) else {
            return;
        };
        let trait_unsafe = self.trait_unsafe.get(&trait_def).copied().unwrap_or(false);
        let trait_name = self.symbol_name(trait_symbol);
        if trait_unsafe && !impl_unsafe {
            self.bag.push(
                self.current_module,
                TypeCheckError::UnsafeTraitRequiresUnsafeImpl { trait_name, span },
            );
        }
        if !trait_unsafe && impl_unsafe {
            let type_name = self.symbol_name(type_name.symbol);
            self.bag.push(
                self.current_module,
                TypeCheckError::UnsafeImplOfSafeTrait { type_name, span },
            );
        }
        let Some(trait_items) = self.find_trait_items(trait_def).map(<[TraitItem]>::to_vec) else {
            return;
        };
        for member in members {
            let ImplMember::Method(m) = member else {
                continue;
            };
            let Some(TraitItem::Method(sig)) = trait_items.iter().find(|item| {
                matches!(
                    item,
                    TraitItem::Method(s) if s.name.symbol == m.name.symbol
                )
            }) else {
                continue;
            };
            let expected_unsafe = Self::trait_method_sig_unsafe(sig, trait_unsafe);
            if trait_unsafe && m.unsafe_ {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::RedundantUnsafeInUnsafeTrait {
                        method: self.symbol_name(m.name.symbol),
                        span: m.name.span,
                    },
                );
            }
            if expected_unsafe != m.unsafe_ && !trait_unsafe {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::Mismatch {
                        expected: if expected_unsafe {
                            format!("`unsafe {}`", self.symbol_name(m.name.symbol))
                        } else {
                            format!("`{}`", self.symbol_name(m.name.symbol))
                        },
                        found: if m.unsafe_ {
                            format!("`unsafe {}`", self.symbol_name(m.name.symbol))
                        } else {
                            format!("`{}`", self.symbol_name(m.name.symbol))
                        },
                        span: m.name.span,
                        kind: MismatchKind::default(),
                    },
                );
            }
        }
    }
    pub(crate) fn check_function_specialized(
        &mut self,
        f: &Function,
        spec_def: DefId,
        base_fn: DefId,
        mono_args: &[TypeId],
    ) {
        let saved_module = self.current_module;
        if let Some(def) = self.resolved.defs.get(base_fn.index() as usize) {
            self.current_module = def.module;
        }
        let saved_impl_self = self.impl_self_type;
        if let Some(type_def) = self.impl_type_for_method(base_fn) {
            let impl_count = self
                .find_inherent_impl_generics(type_def)
                .map(|params| params.len())
                .unwrap_or(0);
            if impl_count > 0 && mono_args.len() >= impl_count {
                let self_ty = self.types.intern(&Ty::Named {
                    def: type_def,
                    args: mono_args[..impl_count].to_vec(),
                });
                self.impl_self_type = Some(self_ty);
            }
        }
        self.mono_template_def = Some(base_fn);

        let combined_generics = self.combined_generic_params_for_fn(base_fn, f);
        let scope_generics = if combined_generics.is_empty() {
            f.generics.as_deref()
        } else {
            Some(combined_generics.as_slice())
        };
        let fn_ty = self.fn_type_for_function_scoped(f, scope_generics);
        let fn_ty = self.mono_apply_fn_subst(fn_ty, mono_args);
        self.value_types.insert(spec_def, fn_ty);
        if let Ty::Fn { ret, .. } = self.types.get(fn_ty).clone() {
            self.fn_ret = Some(ret);
        }
        self.check_function_body(f, spec_def, true, true);
        self.mono_template_def = None;
        self.impl_self_type = saved_impl_self;
        self.current_module = saved_module;
        self.fn_ret = None;
    }

    pub(in crate::typeck::check) fn is_generic_param_type(&self, ty: TypeId) -> bool {
        let Ty::Named { def, args } = self.types.get(ty).clone() else {
            return false;
        };
        args.is_empty()
            && self
                .resolved
                .defs
                .get(def.index() as usize)
                .is_some_and(|d| d.kind == DefKind::GenericParam)
    }

    pub(in crate::typeck::check) fn mono_fix_impl_self_named_type(&mut self, ty: TypeId) -> TypeId {
        let Ty::Named { def, args } = self.types.get(ty).clone() else {
            return ty;
        };
        let Some(self_ty) = self.impl_self_type else {
            return ty;
        };
        let Ty::Named {
            def: self_def,
            args: self_args,
        } = self.types.get(self_ty).clone()
        else {
            return ty;
        };
        if def != self_def || self_args.is_empty() || args.len() != self_args.len() {
            return ty;
        }
        if !args.iter().all(|a| self.is_generic_param_type(*a)) {
            return ty;
        }
        self.types.intern(&Ty::Named {
            def,
            args: self_args,
        })
    }

    pub(in crate::typeck::check) fn mono_substitute_generic_param(
        &mut self,
        ty: TypeId,
        mono_args: &[TypeId],
        ordered_params: Option<&[DefId]>,
    ) -> TypeId {
        if !self.is_generic_param_type(ty) {
            return ty;
        }
        let Ty::Named { def, .. } = self.types.get(ty).clone() else {
            return ty;
        };
        if let Some(subst) = &self.subst {
            if let Some(concrete) = subst.concrete_for_generic(def, self.resolved) {
                return concrete;
            }
        }
        let Some(index) = ordered_params
            .and_then(|defs| Self::generic_param_index_in(def, defs, self.resolved))
            .or_else(|| self.generic_param_index_from_template(def))
        else {
            return ty;
        };
        if let Some(&concrete) = mono_args.get(index) {
            return concrete;
        }
        if let Some(self_ty) = self.impl_self_type {
            if let Ty::Named { args, .. } = self.types.get(self_ty).clone() {
                if let Some(&concrete) = args.get(index) {
                    return concrete;
                }
            }
        }
        ty
    }

    fn generic_param_index_in(
        param_def: DefId,
        param_defs: &[DefId],
        resolved: &ResolvedProgram,
    ) -> Option<usize> {
        if let Some(index) = param_defs.iter().position(|&p| p == param_def) {
            return Some(index);
        }
        let param_record = resolved.defs.get(param_def.index() as usize)?;
        param_defs.iter().position(|&p| {
            let Some(p_record) = resolved.defs.get(p.index() as usize) else {
                return false;
            };
            p_record.kind == DefKind::GenericParam
                && p_record.name == param_record.name
                && p_record.module == param_record.module
        })
    }

    /// Index of `param_def` in the active monomorphization template's generic list.
    fn generic_param_index_from_template(&self, param_def: DefId) -> Option<usize> {
        let template = self.mono_template_def?;
        let param_defs =
            crate::typeck::mono::generic_param_defs_for_fn_base(self.resolved, template)?;
        Self::generic_param_index_in(param_def, &param_defs, self.resolved)
    }

    fn mono_apply_fn_subst(&mut self, fn_ty: TypeId, mono_args: &[TypeId]) -> TypeId {
        let Some(subst) = self.subst.clone() else {
            return fn_ty;
        };
        match self.types.get(fn_ty).clone() {
            Ty::Fn { params, ret } => {
                let mut concrete_params = Vec::with_capacity(params.len());
                for p in &params {
                    let p = Substitution::apply(&mut self.types, *p, &subst, self.resolved);
                    concrete_params.push(self.mono_substitute_generic_param(p, mono_args, None));
                }
                let ret = Substitution::apply(&mut self.types, ret, &subst, self.resolved);
                let ret = self.mono_fix_impl_self_named_type(ret);
                self.types.intern(&Ty::Fn {
                    params: concrete_params,
                    ret,
                })
            }
            other => self.types.intern(&other),
        }
    }

    pub(in crate::typeck::check) fn impl_type_for_method(&self, fn_def: DefId) -> Option<DefId> {
        let base_def = self.resolved.defs.get(fn_def.index() as usize)?;
        for module in &self.resolved.modules {
            if module.id != base_def.module {
                continue;
            }
            for item in &module.program.items {
                if let TopLevelDecl::Impl {
                    type_name, members, ..
                } = &item.inner.decl
                {
                    if members.iter().any(|m| {
                        matches!(m, ImplMember::Method(f) if self.fn_def_for(f) == Some(fn_def))
                    }) {
                        return self
                            .find_def(module.id, type_name.symbol, DefKind::Struct)
                            .or_else(|| self.find_def(module.id, type_name.symbol, DefKind::Enum));
                    }
                }
            }
        }
        None
    }

    pub(in crate::typeck::check) fn def_module(&self, def: DefId) -> u32 {
        self.resolved
            .defs
            .get(def.index() as usize)
            .map(|d| d.module)
            .unwrap_or(self.current_module)
    }

    pub(in crate::typeck::check) fn fn_module(&self, f: &Function) -> u32 {
        self.fn_def_for(f)
            .map(|d| self.def_module(d))
            .unwrap_or(self.current_module)
    }

    pub(in crate::typeck::check) fn fn_type_for_function(&mut self, f: &Function) -> TypeId {
        self.fn_type_for_function_scoped(f, f.generics.as_deref())
    }

    pub(in crate::typeck::check) fn combined_generic_params_for_fn(
        &self,
        base_fn: DefId,
        f: &Function,
    ) -> Vec<phx_syntax::ast::types::GenericParam> {
        let mut params = self
            .impl_type_for_method(base_fn)
            .and_then(|type_def| self.find_inherent_impl_generics(type_def))
            .unwrap_or_default();
        if let Some(generics) = f.generics.as_ref() {
            params.extend(generics.clone());
        }
        params
    }

    pub(in crate::typeck::check) fn fn_type_for_function_scoped(
        &mut self,
        f: &Function,
        scope_generics: Option<&[phx_syntax::ast::types::GenericParam]>,
    ) -> TypeId {
        let module = self.fn_module(f);
        self.with_pushed_generics(module, scope_generics, |this| {
            let type_defs = this.type_defs.clone();
            let ret = f
                .ret
                .as_ref()
                .map(|r| this.lower_ast_type_with_defs(r, &type_defs))
                .unwrap_or(this.unit);
            let params: Vec<_> = f
                .params
                .iter()
                .filter_map(|p| match p {
                    Param::Named { ty, .. } => Some(this.lower_ast_type_with_defs(ty, &type_defs)),
                    Param::Receiver { ty, .. } => ty
                        .as_ref()
                        .map(|t| this.lower_ast_type_with_defs(t, &type_defs)),
                })
                .collect();
            this.types.intern(&Ty::Fn { params, ret })
        })
    }
    pub(in crate::typeck::check) fn check_copyable_drop_conflict(
        &mut self,
        type_def: DefId,
        trait_def: DefId,
        span: Span,
    ) {
        let is_copyable_trait = self.std_trait_kernel.is_copyable_trait(trait_def)
            || crate::typeck::builtins::is_copyable_trait_def(self.resolved, trait_def);
        let is_drop_trait = self.std_trait_kernel.is_drop_trait(trait_def)
            || crate::typeck::builtins::is_drop_trait_def(self.resolved, trait_def);
        let conflicts = if is_copyable_trait {
            implements_drop_for_def(
                &self.program_layout,
                self.resolved,
                &self.std_trait_kernel,
                type_def,
                &[],
            )
        } else if is_drop_trait {
            crate::typeck::builtins::implements_copyable_for_def(
                &self.program_layout,
                self.resolved,
                &self.std_trait_kernel,
                type_def,
                &[],
            )
        } else {
            false
        };
        if conflicts {
            let type_name = self
                .resolved
                .defs
                .get(type_def.index() as usize)
                .map(|d| self.symbol_name(d.name))
                .unwrap_or_else(|| "type".to_owned());
            self.bag.push(
                self.current_module,
                TypeCheckError::CopyableDropConflict { type_name, span },
            );
        }
    }

    pub(in crate::typeck::check) fn impl_self_type_id(
        &mut self,
        type_def: DefId,
        impl_generics: Option<&[phx_syntax::ast::types::GenericParam]>,
    ) -> TypeId {
        let args = impl_generics
            .map(|params| {
                params
                    .iter()
                    .filter_map(|param| {
                        self.find_def(
                            self.current_module,
                            param.name.symbol,
                            DefKind::GenericParam,
                        )
                        .map(|def| self.types.intern(&Ty::Named { def, args: vec![] }))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.types.intern(&Ty::Named {
            def: type_def,
            args,
        })
    }

    pub(in crate::typeck::check) fn is_self_type_name(&self, symbol: Symbol) -> bool {
        self.resolved.interner.resolves_to(symbol, "Self")
    }

    pub(in crate::typeck::check) fn check_impl_decl(
        &mut self,
        type_name: &TypeName,
        trait_: &Option<Node<Type>>,
        generics: Option<&[phx_syntax::ast::types::GenericParam]>,
        members: &[ImplMember],
        span: Span,
    ) {
        let saved_defs = self.type_defs.clone();
        push_generics(
            &mut self.type_defs,
            self.resolved,
            self.current_module,
            generics,
        );
        let impl_type_defs = self.type_defs.clone();
        let saved_trait_impl = self.active_trait_impl;
        let saved_impl_assoc = std::mem::take(&mut self.impl_assoc_types);
        if let (Some(trait_ty), Some(&type_def)) =
            (trait_.as_ref(), self.type_defs.get(&type_name.symbol))
        {
            if let Some((trait_symbol, _)) = trait_bound_head(&trait_ty.inner) {
                if let Some(&trait_def) = self.type_defs.get(&trait_symbol) {
                    self.active_trait_impl = Some((type_def, trait_def));
                    for member in members {
                        if let ImplMember::AssociatedType { name, ty } = member {
                            let concrete = self.lower_ast_type(ty);
                            self.impl_assoc_types.insert(name.symbol, concrete);
                        }
                    }
                }
            }
            self.check_trait_impl_exhaustiveness(type_name, trait_ty, members, span);
        }
        if let Some(&type_def) = self.type_defs.get(&type_name.symbol) {
            let self_ty = self.impl_self_type_id(type_def, generics);
            self.impl_self_type = Some(self_ty);
            for member in members {
                if let ImplMember::Method(m) = member {
                    self.check_function(m);
                }
            }
            if let Some(trait_ty) = trait_.as_ref() {
                if let Some(inst_key) =
                    self.build_trait_inst_key(type_def, vec![], &trait_ty.inner, &impl_type_defs)
                {
                    if let Some(inherited) = self.inherited_by_inst.get(&inst_key).cloned() {
                        self.check_inherited_trait_methods(&inst_key, inherited);
                    }
                }
            }
            self.impl_self_type = None;
        } else {
            for member in members {
                if let ImplMember::Method(m) = member {
                    self.check_function(m);
                }
            }
        }
        self.active_trait_impl = saved_trait_impl;
        self.impl_assoc_types = saved_impl_assoc;
        self.type_defs = saved_defs;
    }

    pub(in crate::typeck::check) fn check_function(&mut self, f: &Function) {
        if !f.derives.is_empty() {
            self.push_unsupported("#derive directive", f.body.span);
        }
        let is_generic = f.generics.as_ref().is_some_and(|g| !g.is_empty());
        if is_generic {
            return;
        }
        let Some(def) = self.fn_def_for(f) else {
            self.push_internal_error(
                "unresolved function definition during type checking",
                f.name.span,
            );
            return;
        };
        if self.impl_type_for_method(def).is_some_and(|type_def| {
            crate::typeck::mono::generic_param_defs_for_type(self.resolved, type_def)
                .is_some_and(|params| !params.is_empty())
        }) {
            self.check_function_body(f, def, true, false);
            return;
        }
        self.check_function_body(f, def, true, true);
    }
}

impl TypeChecker<'_> {
    /// Registers `const` / `var` slots for layout emission without type-checking bodies.
    ///
    /// Generic impl method templates defer body checking to monomorphization; lowering still
    /// needs local slots for annotated bindings in the template AST.
    pub(in crate::typeck::check) fn collect_layout_bindings_block(
        &mut self,
        block: &Block,
        type_defs: &TypeDefMap,
    ) {
        self.enter_scope();
        for item in &block.items {
            match item {
                BlockItem::Stmt(stmt) => match &stmt.inner {
                    Stmt::Const { name, ty, .. } => {
                        let pty = ty
                            .as_ref()
                            .map(|t| self.lower_ast_type_with_defs(t, type_defs))
                            .unwrap_or(self.unit);
                        self.define_local(name.symbol, pty, BindingKind::Const, None, name.span);
                    }
                    Stmt::Var { name, ty, .. } => {
                        let pty = self.lower_ast_type_with_defs(ty, type_defs);
                        self.define_local(name.symbol, pty, BindingKind::Var, None, name.span);
                    }
                    Stmt::Unsafe(body) | Stmt::Loop(body) => {
                        self.collect_layout_bindings_block(&body.inner, type_defs);
                    }
                    Stmt::While { body, .. } => {
                        self.collect_layout_bindings_block(&body.inner, type_defs);
                    }
                    Stmt::Expr(expr) => {
                        self.collect_layout_bindings_from_expr(expr, type_defs);
                    }
                    Stmt::Assign { .. }
                    | Stmt::Return(_)
                    | Stmt::Break { .. }
                    | Stmt::Continue { .. }
                    | Stmt::ForIn { .. } => {}
                },
                BlockItem::Expr(expr) => {
                    self.collect_layout_bindings_from_expr(expr, type_defs);
                }
                BlockItem::Import(_) => {}
            }
        }
        self.exit_scope();
    }

    pub(in crate::typeck::check) fn collect_layout_bindings_from_expr(
        &mut self,
        expr: &ExprNode,
        type_defs: &TypeDefMap,
    ) {
        match &expr.inner {
            Expr::Block(block) | Expr::Unsafe(block) => {
                self.collect_layout_bindings_block(&block.inner, type_defs);
            }
            Expr::If {
                then_block,
                else_ifs,
                else_block,
                ..
            } => {
                self.collect_layout_bindings_block(&then_block.inner, type_defs);
                for (_, block) in else_ifs {
                    self.collect_layout_bindings_block(&block.inner, type_defs);
                }
                if let Some(else_b) = else_block {
                    self.collect_layout_bindings_block(&else_b.inner, type_defs);
                }
            }
            _ => {}
        }
    }
}

/// Returns whether `block` references the implicit impl receiver (`self`).
pub(in crate::typeck::check) fn function_body_uses_impl_receiver(block: &Block) -> bool {
    block.items.iter().any(block_item_uses_impl_receiver)
}

fn block_item_uses_impl_receiver(item: &BlockItem) -> bool {
    match item {
        BlockItem::Stmt(stmt) => stmt_uses_impl_receiver(&stmt.inner),
        BlockItem::Expr(expr) => expr_uses_impl_receiver(expr),
        BlockItem::Import(_) => false,
    }
}

fn stmt_uses_impl_receiver(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Const { init, .. } | Stmt::Var { init, .. } => expr_uses_impl_receiver(init),
        Stmt::Assign { expr } | Stmt::Expr(expr) => expr_uses_impl_receiver(expr),
        Stmt::Return(value) => value.as_ref().is_some_and(expr_uses_impl_receiver),
        Stmt::Break { value, .. } => value.as_ref().is_some_and(expr_uses_impl_receiver),
        Stmt::While { cond, body, .. } => {
            expr_uses_impl_receiver(cond) || function_body_uses_impl_receiver(&body.inner)
        }
        Stmt::ForIn { iter, body, .. } => {
            expr_uses_impl_receiver(iter) || function_body_uses_impl_receiver(&body.inner)
        }
        Stmt::Loop(body) | Stmt::Unsafe(body) => function_body_uses_impl_receiver(&body.inner),
        Stmt::Continue { .. } => false,
    }
}

fn expr_uses_impl_receiver(expr: &ExprNode) -> bool {
    match &expr.inner {
        Expr::Ident(ident) => ident.symbol == impl_receiver_symbol(),
        Expr::Unary { operand, .. } => expr_uses_impl_receiver(operand),
        Expr::Binary { left, right, .. } => {
            expr_uses_impl_receiver(left) || expr_uses_impl_receiver(right)
        }
        Expr::Assign { target, value, .. } => {
            expr_uses_impl_receiver(target) || expr_uses_impl_receiver(value)
        }
        Expr::Cast { expr: inner, .. } => expr_uses_impl_receiver(inner),
        Expr::Postfix { base, ops } => {
            if expr_uses_impl_receiver(base) {
                return true;
            }
            ops.iter().any(|op| match op {
                PostfixOp::Call { args, .. } | PostfixOp::Method { args, .. } => {
                    args.iter().any(expr_uses_impl_receiver)
                }
                PostfixOp::Index(idx) => expr_uses_impl_receiver(idx),
                PostfixOp::Field { .. } => false,
                PostfixOp::Try => false,
            })
        }
        Expr::If {
            condition,
            then_block,
            else_ifs,
            else_block,
        } => {
            if_condition_uses_impl_receiver(condition)
                || function_body_uses_impl_receiver(&then_block.inner)
                || else_ifs.iter().any(|(c, b)| {
                    if_condition_uses_impl_receiver(c) || function_body_uses_impl_receiver(&b.inner)
                })
                || else_block
                    .as_ref()
                    .is_some_and(|b| function_body_uses_impl_receiver(&b.inner))
        }
        Expr::Match { scrutinee, arms } => {
            expr_uses_impl_receiver(scrutinee)
                || arms.iter().any(|arm| expr_uses_impl_receiver(&arm.body))
        }
        Expr::Block(block) | Expr::Unsafe(block) => function_body_uses_impl_receiver(&block.inner),
        Expr::StructLit { fields, .. } => fields.iter().any(|field| match field {
            StructFieldInit::Field { value, .. } => expr_uses_impl_receiver(value),
            StructFieldInit::Spread(base) => expr_uses_impl_receiver(base),
        }),
        Expr::Lambda { body, .. } => match body {
            LambdaBody::Expr(e) => expr_uses_impl_receiver(e),
            LambdaBody::Block(b) => function_body_uses_impl_receiver(&b.inner),
        },
        Expr::Tuple(items) | Expr::Array(items) => items.iter().any(expr_uses_impl_receiver),
        Expr::Range { start, end, .. } => {
            expr_uses_impl_receiver(start) || expr_uses_impl_receiver(end)
        }
        Expr::RuntimeDirective { args, .. } => args.iter().any(expr_uses_impl_receiver),
        Expr::Literal(_) | Expr::Path(_) => false,
    }
}

fn if_condition_uses_impl_receiver(condition: &IfCondition) -> bool {
    match condition {
        IfCondition::Bool(expr) => expr_uses_impl_receiver(expr),
        IfCondition::Pattern { scrutinee, .. } => expr_uses_impl_receiver(scrutinee),
    }
}

pub(in crate::typeck::check) fn find_trait_method_def(
    layout: &ProgramLayout,
    type_def: DefId,
    implementer_args: &[TypeId],
    method: Symbol,
) -> Option<DefId> {
    let mut matches: Vec<DefId> = layout
        .trait_methods
        .iter()
        .filter(|((key, m), _)| {
            key.implementer == type_def
                && key.implementer_args.as_slice() == implementer_args
                && *m == method
        })
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

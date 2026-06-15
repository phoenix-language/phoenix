//! Statement, block, loop, and drop planning.

use phx_diagnostics::{MismatchKind, Span, TypeCheckError};
use phx_syntax::ast::decl::{Function, Param};
use phx_syntax::ast::expr::{Expr, IfCondition, LambdaBody, PostfixOp, StructFieldInit, UnaryOp};
use phx_syntax::ast::ident::Ident;
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};
use phx_syntax::ast::{BlockNode, ExprNode};
use phx_syntax::{Symbol, for_in_iter_symbol, impl_receiver_symbol};

use super::TypeChecker;
use super::decl::trailing_value_expr;
use super::impls::{find_trait_method_def, function_body_uses_impl_receiver};
use crate::resolver::DefId;
use crate::typeck::bindings::{BindingKind, DropEvent, ForInPlan, FunctionLayoutBuilder};
use crate::typeck::bounds::type_satisfies_trait_inst;
use crate::typeck::builtins::{implements_drop, resolve_drop_fn};
use crate::typeck::display::format_type_diagnostic;
use crate::typeck::layout::TraitInstKey;
use crate::typeck::lower_ty::push_generics;
use crate::typeck::ownership::OwnershipTracker;
use crate::typeck::primitive::primitive_kind_for_type;
use crate::typeck::types::{Ty, TypeId, TypeInterner};
use phx_bytecode::{SLOT_KIND_AGG, SLOT_KIND_FN_PTR};

impl TypeChecker<'_> {
    pub(in crate::typeck::check) fn error_loop_control_outside_loop(
        &mut self,
        keyword: &'static str,
        span: Span,
    ) {
        self.bag.push(
            self.current_module,
            TypeCheckError::LoopControlOutsideLoop { keyword, span },
        );
    }

    pub(in crate::typeck::check) fn with_loop_body<F: FnOnce(&mut Self)>(
        &mut self,
        body: &Block,
        f: F,
    ) {
        self.loop_depth = self.loop_depth.saturating_add(1);
        let body_scope = self.layout_scope_depth();
        self.loop_body_scope_depths.push(body_scope);
        let pre = self.ownership.clone();
        let ((), body_end) = self.check_with_ownership_fork(&pre, f);
        let loop_head = OwnershipTracker::join_arms(&pre, &[body_end]);
        self.check_loop_back_edge_uses(&pre, &loop_head, body);
        self.ownership = loop_head;
        self.loop_body_scope_depths.pop();
        self.loop_depth = self.loop_depth.saturating_sub(1);
    }

    pub(in crate::typeck::check) fn check_loop_back_edge_uses(
        &mut self,
        pre: &OwnershipTracker,
        loop_head: &OwnershipTracker,
        body: &Block,
    ) {
        for (symbol, _depth, move_span) in OwnershipTracker::newly_moved_since(pre, loop_head) {
            let mut use_spans = Vec::new();
            self.collect_ident_read_uses_in_block(body, symbol, &mut use_spans);
            let name = self.symbol_name(symbol);
            for span in use_spans {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::UseAfterMove {
                        name: name.clone(),
                        move_span,
                        span,
                    },
                );
            }
        }
    }

    pub(in crate::typeck::check) fn collect_ident_read_uses_in_block(
        &self,
        block: &Block,
        symbol: Symbol,
        out: &mut Vec<Span>,
    ) {
        for item in &block.items {
            match item {
                BlockItem::Expr(expr) => {
                    self.collect_ident_read_uses_in_expr(&expr.inner, symbol, out);
                }
                BlockItem::Stmt(stmt) => {
                    self.collect_ident_read_uses_in_stmt(&stmt.inner, symbol, out);
                }
                BlockItem::Import(_) => {}
            }
        }
    }

    pub(in crate::typeck::check) fn collect_ident_read_uses_in_stmt(
        &self,
        stmt: &Stmt,
        symbol: Symbol,
        out: &mut Vec<Span>,
    ) {
        match stmt {
            Stmt::Const { init, .. } | Stmt::Var { init, .. } => {
                if !Self::expr_is_move_source_ident(init, symbol) {
                    self.collect_ident_read_uses_in_expr(&init.inner, symbol, out);
                }
            }
            Stmt::Assign { expr } => {
                self.collect_ident_read_uses_in_expr(&expr.inner, symbol, out);
            }
            Stmt::Expr(expr) | Stmt::Return(Some(expr)) => {
                self.collect_ident_read_uses_in_expr(&expr.inner, symbol, out);
            }
            Stmt::Return(None) | Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::While { cond, body } => {
                self.collect_ident_read_uses_in_expr(&cond.inner, symbol, out);
                self.collect_ident_read_uses_in_block(&body.inner, symbol, out);
            }
            Stmt::ForIn { iter, body, .. } => {
                self.collect_ident_read_uses_in_expr(&iter.inner, symbol, out);
                self.collect_ident_read_uses_in_block(&body.inner, symbol, out);
            }
            Stmt::Loop(body) | Stmt::Unsafe(body) => {
                self.collect_ident_read_uses_in_block(&body.inner, symbol, out);
            }
        }
    }

    pub(in crate::typeck::check) fn expr_is_move_source_ident(
        expr: &ExprNode,
        symbol: Symbol,
    ) -> bool {
        matches!(&expr.inner, Expr::Ident(ident) if ident.symbol == symbol)
    }

    pub(in crate::typeck::check) fn collect_ident_read_uses_in_expr(
        &self,
        expr: &Expr,
        symbol: Symbol,
        out: &mut Vec<Span>,
    ) {
        match expr {
            Expr::Ident(ident) if ident.symbol == symbol => out.push(ident.span),
            Expr::Ident(_) => {}
            Expr::Literal(_) | Expr::Path(_) => {}
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    self.collect_ident_read_uses_in_expr(&item.inner, symbol, out);
                }
            }
            Expr::Unary { operand, .. } => {
                self.collect_ident_read_uses_in_expr(&operand.inner, symbol, out);
            }
            Expr::Binary { left, right, .. } => {
                self.collect_ident_read_uses_in_expr(&left.inner, symbol, out);
                self.collect_ident_read_uses_in_expr(&right.inner, symbol, out);
            }
            Expr::Assign { target, value, .. } => {
                self.collect_ident_read_uses_in_assign_target(&target.inner, symbol, out);
                self.collect_ident_read_uses_in_expr(&value.inner, symbol, out);
            }
            Expr::Cast { expr, .. } => {
                self.collect_ident_read_uses_in_expr(&expr.inner, symbol, out);
            }
            Expr::Postfix { base, ops } => {
                self.collect_ident_read_uses_in_postfix(&base.inner, ops, symbol, out);
            }
            Expr::If {
                condition,
                then_block,
                else_ifs,
                else_block,
            } => {
                self.collect_ident_read_uses_in_if_expr(
                    condition.as_ref(),
                    then_block,
                    else_ifs,
                    else_block.as_ref(),
                    symbol,
                    out,
                );
            }
            Expr::Match { scrutinee, arms } => {
                self.collect_ident_read_uses_in_expr(&scrutinee.inner, symbol, out);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        self.collect_ident_read_uses_in_expr(&guard.inner, symbol, out);
                    }
                    self.collect_ident_read_uses_in_expr(&arm.body.inner, symbol, out);
                }
            }
            Expr::Block(block) | Expr::Unsafe(block) => {
                self.collect_ident_read_uses_in_block(&block.inner, symbol, out);
            }
            Expr::StructLit { fields, .. } => {
                for field in fields {
                    match field {
                        StructFieldInit::Field { value, .. } => {
                            self.collect_ident_read_uses_in_expr(&value.inner, symbol, out);
                        }
                        StructFieldInit::Spread(base) => {
                            self.collect_ident_read_uses_in_expr(&base.inner, symbol, out);
                        }
                    }
                }
            }
            Expr::Range { start, end, .. } => {
                self.collect_ident_read_uses_in_expr(&start.inner, symbol, out);
                self.collect_ident_read_uses_in_expr(&end.inner, symbol, out);
            }
            Expr::Lambda { body, .. } => match body {
                LambdaBody::Expr(expr) => {
                    self.collect_ident_read_uses_in_expr(&expr.inner, symbol, out);
                }
                LambdaBody::Block(block) => {
                    self.collect_ident_read_uses_in_block(&block.inner, symbol, out);
                }
            },
            Expr::RuntimeDirective { args, .. } => {
                for arg in args {
                    if !Self::expr_is_move_source_ident(arg, symbol) {
                        self.collect_ident_read_uses_in_expr(&arg.inner, symbol, out);
                    }
                }
            }
        }
    }

    pub(in crate::typeck::check) fn collect_ident_read_uses_in_postfix(
        &self,
        base: &Expr,
        ops: &[PostfixOp],
        symbol: Symbol,
        out: &mut Vec<Span>,
    ) {
        self.collect_ident_read_uses_in_expr(base, symbol, out);
        for op in ops {
            match op {
                PostfixOp::Field(_) | PostfixOp::Try => {}
                PostfixOp::Method { args, .. } | PostfixOp::Call { args, .. } => {
                    for arg in args {
                        if !Self::expr_is_move_source_ident(arg, symbol) {
                            self.collect_ident_read_uses_in_expr(&arg.inner, symbol, out);
                        }
                    }
                }
                PostfixOp::Index(index) => {
                    self.collect_ident_read_uses_in_expr(&index.inner, symbol, out);
                }
            }
        }
    }

    pub(in crate::typeck::check) fn collect_ident_read_uses_in_if_expr(
        &self,
        condition: &IfCondition,
        then_block: &BlockNode,
        else_ifs: &[(IfCondition, BlockNode)],
        else_block: Option<&BlockNode>,
        symbol: Symbol,
        out: &mut Vec<Span>,
    ) {
        self.collect_ident_read_uses_in_if_condition(condition, symbol, out);
        self.collect_ident_read_uses_in_block(&then_block.inner, symbol, out);
        for (cond, block) in else_ifs {
            self.collect_ident_read_uses_in_if_condition(cond, symbol, out);
            self.collect_ident_read_uses_in_block(&block.inner, symbol, out);
        }
        if let Some(block) = else_block {
            self.collect_ident_read_uses_in_block(&block.inner, symbol, out);
        }
    }

    pub(in crate::typeck::check) fn collect_ident_read_uses_in_if_condition(
        &self,
        condition: &IfCondition,
        symbol: Symbol,
        out: &mut Vec<Span>,
    ) {
        match condition {
            IfCondition::Bool(cond) => {
                self.collect_ident_read_uses_in_expr(&cond.inner, symbol, out);
            }
            IfCondition::Pattern { scrutinee, .. } => {
                self.collect_ident_read_uses_in_expr(&scrutinee.inner, symbol, out);
            }
        }
    }

    pub(in crate::typeck::check) fn collect_ident_read_uses_in_assign_target(
        &self,
        expr: &Expr,
        symbol: Symbol,
        out: &mut Vec<Span>,
    ) {
        match expr {
            Expr::Ident(_) => {}
            Expr::Postfix { base, ops } => {
                self.collect_ident_read_uses_in_expr(&base.inner, symbol, out);
                for op in ops {
                    if let PostfixOp::Index(index) = op {
                        self.collect_ident_read_uses_in_expr(&index.inner, symbol, out);
                    }
                }
            }
            Expr::Unary {
                op: UnaryOp::Deref,
                operand,
            } => {
                self.collect_ident_read_uses_in_expr(&operand.inner, symbol, out);
            }
            Expr::Tuple(items) => {
                for item in items {
                    self.collect_ident_read_uses_in_assign_target(&item.inner, symbol, out);
                }
            }
            Expr::Binary { left, right, .. } => {
                self.collect_ident_read_uses_in_assign_target(&left.inner, symbol, out);
                self.collect_ident_read_uses_in_assign_target(&right.inner, symbol, out);
            }
            _ => {}
        }
    }

    pub(in crate::typeck::check) fn layout_scope_depth(&self) -> u32 {
        self.layout
            .as_ref()
            .map_or(0, FunctionLayoutBuilder::scope_depth)
    }

    pub(in crate::typeck::check) fn plan_drops_for_scope_depths(
        &mut self,
        from_depth: u32,
        to_depth: u32,
    ) {
        if from_depth < to_depth {
            return;
        }
        for depth in (to_depth..=from_depth).rev() {
            self.plan_drops_at_scope_depth(depth);
        }
    }

    pub(in crate::typeck::check) fn plan_drops_at_scope_depth(&mut self, depth: u32) {
        let candidates: Vec<_> = self
            .layout
            .as_ref()
            .map(|layout| {
                let mut bindings: Vec<_> = layout
                    .bindings_at_depth(depth)
                    .into_iter()
                    .filter(|b| b.kind != BindingKind::MatchTemp)
                    .collect();
                bindings.sort_by_key(|b| b.slot.index());
                bindings
            })
            .unwrap_or_default();
        let mut planned = Vec::new();
        for binding in candidates.into_iter().rev() {
            if self.ownership.moved_at(binding.symbol).is_some() {
                continue;
            }
            if binding.kind == BindingKind::Param {
                if let Some(fn_def) = self.layout.as_ref().map(FunctionLayoutBuilder::def) {
                    if self.fn_is_drop_method(fn_def) {
                        continue;
                    }
                }
            }
            if !implements_drop(
                &self.types,
                &self.program_layout,
                self.resolved,
                &self.std_trait_kernel,
                binding.ty,
            ) {
                continue;
            }
            let Ty::Named { def, args } = self.types.get(binding.ty).clone() else {
                continue;
            };
            let Some(drop_fn) = resolve_drop_fn(
                &self.program_layout,
                self.resolved,
                &self.std_trait_kernel,
                def,
                &args,
            ) else {
                continue;
            };
            let prim_kind = drop_prim_kind_byte(&self.types, binding.ty);
            planned.push(DropEvent {
                scope_depth: depth,
                slot: binding.slot,
                ty: binding.ty,
                drop_fn,
                prim_kind,
                span: binding.declare_span,
            });
        }
        if let Some(layout) = &mut self.layout {
            for event in planned {
                layout.plan_drop(event);
            }
        }
    }

    pub(in crate::typeck::check) fn mark_method_receiver_moved(
        &mut self,
        receiver_expr: &ExprNode,
        receiver_ty: TypeId,
        fn_def: DefId,
    ) {
        let Expr::Ident(ident) = &receiver_expr.inner else {
            return;
        };
        if !self.method_consumes_receiver(fn_def, receiver_ty) {
            return;
        }
        if !self.is_copyable_ty(receiver_ty) {
            self.ownership
                .move_binding(ident.symbol, receiver_expr.span);
        }
    }

    pub(in crate::typeck::check) fn method_consumes_receiver(
        &self,
        fn_def: DefId,
        receiver_ty: TypeId,
    ) -> bool {
        let Some(f) = self.find_function_decl(fn_def) else {
            return false;
        };
        for param in &f.params {
            let Param::Receiver { ty, .. } = param else {
                continue;
            };
            return match ty {
                None => true,
                Some(t) => !matches!(
                    t.inner,
                    phx_syntax::ast::types::Type::Ref { .. }
                        | phx_syntax::ast::types::Type::Ptr { .. }
                ),
            };
        }
        let Some(&fn_ty) = self.value_types.get(&fn_def) else {
            return false;
        };
        let Ty::Fn { params, .. } = self.types.get(fn_ty).clone() else {
            return false;
        };
        let Some(first) = params.first() else {
            return false;
        };
        if matches!(self.types.get(*first), Ty::Ref { .. } | Ty::Ptr { .. }) {
            return false;
        }
        self.method_receiver_matches(*first, receiver_ty)
    }

    pub(in crate::typeck::check) fn fn_is_drop_method(&self, fn_def: DefId) -> bool {
        self.program_layout
            .trait_methods
            .iter()
            .any(|((key, method), &def)| {
                def == fn_def
                    && self.std_trait_kernel.is_drop_trait(key.trait_def)
                    && self.resolved.interner.resolves_to(*method, "drop")
            })
    }

    #[allow(clippy::too_many_lines)]
    pub(in crate::typeck::check) fn check_function_body(
        &mut self,
        f: &Function,
        def: DefId,
        emit_layout: bool,
        check_body: bool,
    ) {
        if self.intrinsic_kernel.is_intrinsic_fn(def) {
            return;
        }
        let module = self.def_module(def);
        let saved_type_defs = self.type_defs.clone();
        push_generics(
            &mut self.type_defs,
            &self.resolved.defs,
            module,
            f.generics.as_deref(),
        );
        let type_defs = self.type_defs.clone();
        let ret = self.fn_ret.unwrap_or_else(|| {
            f.ret
                .as_ref()
                .map(|r| self.lower_ast_type_with_defs(r, &type_defs))
                .unwrap_or(self.unit)
        });
        self.fn_ret = Some(ret);
        self.ownership = OwnershipTracker::new();
        if emit_layout {
            self.layout = Some(FunctionLayoutBuilder::new(def, ret));
        }
        let expr_start = self.next_expr;
        let has_receiver = f.params.iter().any(|p| matches!(p, Param::Receiver { .. }));
        let specialized_param_types = self.subst.as_ref().and_then(|_| {
            self.value_types.get(&def).and_then(|&fn_ty| {
                if let Ty::Fn { params, .. } = self.types.get(fn_ty).clone() {
                    Some(params)
                } else {
                    None
                }
            })
        });
        let mut param_index = 0usize;
        for p in &f.params {
            match p {
                Param::Named { name, ty, .. } => {
                    let pty = specialized_param_types
                        .as_ref()
                        .and_then(|params| params.get(param_index).copied())
                        .unwrap_or_else(|| self.lower_ast_type_with_defs(ty, &type_defs));
                    param_index += 1;
                    self.define_local(name.symbol, pty, BindingKind::Param, None, name.span);
                }
                Param::Receiver { ty, span, .. } => {
                    let pty = specialized_param_types
                        .as_ref()
                        .and_then(|params| params.get(param_index).copied())
                        .or_else(|| {
                            ty.as_ref()
                                .map(|t| self.lower_ast_type_with_defs(t, &type_defs))
                        })
                        .or(self.impl_self_type)
                        .unwrap_or(self.unit);
                    param_index += 1;
                    self.define_local(impl_receiver_symbol(), pty, BindingKind::Param, None, *span);
                }
            }
        }
        if self.impl_self_type.is_some() && !has_receiver {
            let needs_implicit_self = function_body_uses_impl_receiver(&f.body.inner);
            if needs_implicit_self {
                if let Some(self_ty) = self.impl_self_type {
                    self.define_local(
                        impl_receiver_symbol(),
                        self_ty,
                        BindingKind::Param,
                        None,
                        f.name.span,
                    );
                }
            }
        }
        if emit_layout && !check_body {
            self.collect_layout_bindings_block(&f.body.inner, &type_defs);
        }
        if check_body {
            let check_body = |this: &mut Self| {
                let body_ty = this.check_block_value(&f.body.inner);
                if !this.types_equal(body_ty, ret) {
                    this.error_mismatch(ret, body_ty, f.body.span, MismatchKind::FunctionBody);
                }
                if this.is_borrow_type(body_ty) {
                    if let Some(expr) = trailing_value_expr(&f.body.inner) {
                        this.check_expr_escapes_local(expr);
                    }
                }
            };
            if self.is_effective_unsafe(def) {
                self.with_unsafe(|this| check_body(this));
            } else {
                check_body(self);
            }
        }
        if emit_layout {
            self.plan_drops_at_scope_depth(0);
            if let Some(mut builder) = self.layout.take() {
                builder.set_expr_range(expr_start, self.next_expr);
                self.functions.push(builder.finish());
            }
        }
        self.type_defs = saved_type_defs;
        self.fn_ret = None;
        self.layout = None;
    }

    pub(in crate::typeck::check) fn check_block(&mut self, block: &Block) {
        let _ = self.check_block_value(block);
    }

    /// Type-checks `block` and returns the type of its last value-producing item.
    pub(in crate::typeck::check) fn check_block_value(&mut self, block: &Block) -> TypeId {
        self.enter_scope();
        let mut last = self.unit;
        for item in &block.items {
            last = match item {
                BlockItem::Stmt(stmt) => self.check_block_stmt_value(stmt),
                BlockItem::Expr(expr) => self.check_expr_node(expr),
                BlockItem::Import(_) => self.unit,
            };
        }
        self.exit_scope();
        last
    }

    pub(in crate::typeck::check) fn check_block_stmt_value(
        &mut self,
        stmt: &phx_syntax::ast::StmtNode,
    ) -> TypeId {
        match &stmt.inner {
            Stmt::Expr(expr) => {
                if matches!(expr.inner, Expr::Assign { .. }) {
                    let _ = self.check_expr_node(expr);
                    self.unit
                } else {
                    self.check_expr_node(expr)
                }
            }
            Stmt::Return(expr) => self.check_return(stmt.span, expr.as_ref()),
            _ => {
                self.check_stmt(stmt);
                self.unit
            }
        }
    }

    pub(in crate::typeck::check) fn check_return(
        &mut self,
        span: Span,
        expr: Option<&ExprNode>,
    ) -> TypeId {
        let current = self.layout_scope_depth();
        self.plan_drops_for_scope_depths(current, 0);
        if let Some(e) = expr {
            let saved_ctor = self.ctor_expected;
            self.ctor_expected = self.fn_ret;
            let got = self.check_expr_node(e);
            self.ctor_expected = saved_ctor;
            if self.is_borrow_type(got) {
                self.check_expr_escapes_local(e);
            }
            if let Some(ret) = self.fn_ret {
                if !self.types_equal(got, ret) {
                    self.error_mismatch(ret, got, e.span, MismatchKind::Return);
                }
            }
            got
        } else {
            if let Some(ret) = self.fn_ret {
                if !self.types_equal(ret, self.unit) {
                    self.error_mismatch(self.unit, ret, span, MismatchKind::Return);
                }
            }
            self.unit
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(in crate::typeck::check) fn check_stmt(&mut self, stmt: &phx_syntax::ast::StmtNode) {
        match &stmt.inner {
            Stmt::Const { name, ty, init } => {
                let expected = ty.as_ref().map(|t| self.lower_ast_type(t));
                self.ctor_expected = expected;
                let got = self.check_expr_node(init);
                self.ctor_expected = None;
                if let Some(expected) = expected {
                    if !self.types_equal(got, expected) {
                        if let Some(t) = ty.as_ref() {
                            self.error_mismatch(
                                expected,
                                got,
                                init.span,
                                MismatchKind::ConstBinding {
                                    name: self.symbol_name(name.symbol),
                                    annotation_span: t.span,
                                },
                            );
                        }
                    }
                }
                self.define_local(
                    name.symbol,
                    got,
                    BindingKind::Const,
                    Some(&init.inner),
                    name.span,
                );
            }
            Stmt::Var { name, ty, init } => {
                let expected = self.lower_ast_type(ty);
                self.ctor_expected = Some(expected);
                let got = self.check_expr_node(init);
                self.ctor_expected = None;
                if !self.types_equal(got, expected) {
                    self.error_mismatch(
                        expected,
                        got,
                        init.span,
                        MismatchKind::VarBinding {
                            name: self.symbol_name(name.symbol),
                            annotation_span: ty.span,
                        },
                    );
                }
                self.move_if_non_copyable(init, got);
                self.define_local(
                    name.symbol,
                    expected,
                    BindingKind::Var,
                    Some(&init.inner),
                    name.span,
                );
            }
            Stmt::Assign { expr } => {
                let _ = self.check_expr_node(expr);
            }
            Stmt::Expr(expr) => {
                let _ = self.check_expr_node(expr);
            }
            Stmt::Return(expr) => {
                let _ = self.check_return(stmt.span, expr.as_ref());
            }
            Stmt::Break { value, span } => {
                if self.loop_depth == 0 {
                    self.error_loop_control_outside_loop("break", *span);
                } else if value.is_some() {
                    self.push_unsupported("break with value", *span);
                } else {
                    let current = self.layout_scope_depth();
                    let loop_body = self
                        .loop_body_scope_depths
                        .last()
                        .copied()
                        .map_or(0, |d| d.saturating_add(1));
                    self.plan_drops_for_scope_depths(current, loop_body);
                }
            }
            Stmt::Continue { span } if self.loop_depth == 0 => {
                self.error_loop_control_outside_loop("continue", *span);
            }
            Stmt::Continue { .. } => {}
            Stmt::While { cond, body } => {
                let c = self.check_expr_node(cond);
                if !self.types_equal(c, self.bool_ty) {
                    self.error_mismatch(self.bool_ty, c, cond.span, MismatchKind::Condition);
                }
                self.with_loop_body(&body.inner, |this| this.check_block(&body.inner));
            }
            Stmt::ForIn {
                binding,
                iter,
                body,
            } => {
                self.check_for_in(*binding, iter, &body.inner, binding.span);
            }
            Stmt::Loop(body) => {
                self.with_loop_body(&body.inner, |this| this.check_block(&body.inner));
            }
            Stmt::Unsafe(body) => self.with_unsafe(|this| this.check_block(&body.inner)),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(in crate::typeck::check) fn check_for_in(
        &mut self,
        binding: Ident,
        iter: &ExprNode,
        body: &Block,
        span: Span,
    ) {
        let iter_ty = self.check_expr_node(iter);
        if let Expr::Ident(ident) = &iter.inner {
            if !self.is_copyable_ty(iter_ty) {
                self.ownership.move_binding(ident.symbol, iter.span);
            }
        }
        let Some((implementer, implementer_args)) = self.named_type_args(iter_ty) else {
            let type_name = format_type_diagnostic(
                &self.types,
                &self.resolved.interner,
                &self.resolved.defs,
                iter_ty,
            );
            self.bag.push(
                self.current_module,
                TypeCheckError::TraitNotSatisfied {
                    type_name,
                    trait_name: "IntoIter".to_owned(),
                    span: iter.span,
                },
            );
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        let Some(item_sym) = self.symbol_named("Item") else {
            self.push_unsupported("IntoIter::Item associated type", span);
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        let Some(into_iter_assoc_sym) = self.symbol_named("IntoIter") else {
            self.push_unsupported("IntoIter::IntoIter associated type", span);
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        let Some(into_iter_fn_sym) = self.symbol_named("into_iter") else {
            self.push_unsupported("IntoIter::into_iter", span);
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        let Some(next_fn_sym) = self.symbol_named("next") else {
            self.push_unsupported("Iterator::next", span);
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        let Some(into_iter_trait) = self.resolve_trait_def_by_name("IntoIter") else {
            self.push_unsupported("IntoIter trait (import std::core::iter)", span);
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        let Some(iterator_trait) = self.resolve_trait_def_by_name("Iterator") else {
            self.push_unsupported("Iterator trait (import std::core::iter)", span);
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        if !type_satisfies_trait_inst(
            &self.program_layout,
            &self.types,
            &self.std_trait_kernel,
            iter_ty,
            into_iter_trait,
            &[],
            Some(&self.alias_env()),
        ) {
            let type_name = format_type_diagnostic(
                &self.types,
                &self.resolved.interner,
                &self.resolved.defs,
                iter_ty,
            );
            self.bag.push(
                self.current_module,
                TypeCheckError::TraitNotSatisfied {
                    type_name,
                    trait_name: "IntoIter".to_owned(),
                    span: iter.span,
                },
            );
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        }
        let into_key = TraitInstKey::new(
            implementer,
            implementer_args.clone(),
            into_iter_trait,
            vec![],
        );
        let Some(item_ty) = self
            .program_layout
            .trait_assoc_impls
            .get(&(into_key.clone(), item_sym))
            .copied()
        else {
            self.bag.push(
                self.current_module,
                TypeCheckError::UnsupportedFeature {
                    feature: "IntoIter without Item associated type",
                    span,
                },
            );
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        let Some(iter_state_ty) = self
            .program_layout
            .trait_assoc_impls
            .get(&(into_key, into_iter_assoc_sym))
            .copied()
        else {
            self.bag.push(
                self.current_module,
                TypeCheckError::UnsupportedFeature {
                    feature: "IntoIter without IntoIter associated type",
                    span,
                },
            );
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        let Some(into_iter_fn) = find_trait_method_def(
            &self.program_layout,
            implementer,
            &implementer_args,
            into_iter_fn_sym,
        ) else {
            self.bag.push(
                self.current_module,
                TypeCheckError::UnsupportedFeature {
                    feature: "IntoIter::into_iter",
                    span,
                },
            );
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        let Some((state_def, state_args)) = self.named_type_args(iter_state_ty) else {
            self.bag.push(
                self.current_module,
                TypeCheckError::TraitNotSatisfied {
                    type_name: format_type_diagnostic(
                        &self.types,
                        &self.resolved.interner,
                        &self.resolved.defs,
                        iter_state_ty,
                    ),
                    trait_name: "Iterator".to_owned(),
                    span,
                },
            );
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        if !type_satisfies_trait_inst(
            &self.program_layout,
            &self.types,
            &self.std_trait_kernel,
            iter_state_ty,
            iterator_trait,
            &[],
            Some(&self.alias_env()),
        ) {
            let type_name = format_type_diagnostic(
                &self.types,
                &self.resolved.interner,
                &self.resolved.defs,
                iter_state_ty,
            );
            self.bag.push(
                self.current_module,
                TypeCheckError::TraitNotSatisfied {
                    type_name,
                    trait_name: "Iterator".to_owned(),
                    span,
                },
            );
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        }
        let iter_key = TraitInstKey::new(state_def, state_args.clone(), iterator_trait, vec![]);
        let iter_item_ty = self
            .program_layout
            .trait_assoc_impls
            .get(&(iter_key, item_sym))
            .copied()
            .unwrap_or(item_ty);
        if !self.types_equal(item_ty, iter_item_ty) {
            self.error_mismatch(item_ty, iter_item_ty, span, MismatchKind::default());
        }
        let Some(next_fn) =
            find_trait_method_def(&self.program_layout, state_def, &state_args, next_fn_sym)
        else {
            self.bag.push(
                self.current_module,
                TypeCheckError::UnsupportedFeature {
                    feature: "Iterator::next",
                    span,
                },
            );
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        let Some(option_ty) = self.option_ty_for_item(item_ty) else {
            self.push_unsupported(
                "for-in requires std Option (import std::core::option)",
                span,
            );
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        let Some(some_variant) = self.some_variant_symbol(option_ty) else {
            self.push_unsupported("Option::Some variant for for-loop", span);
            self.with_loop_body(body, |this| this.check_block(body));
            return;
        };
        let (iter_temp_slot, option_match_temp) = if let Some(layout) = &mut self.layout {
            let plan_index = layout.next_for_in_plan_index();
            let iter_temp_slot = layout.alloc(
                for_in_iter_symbol(plan_index),
                iter_state_ty,
                BindingKind::Var,
                None,
                span,
            );
            let option_match_temp = layout.alloc_match_scrutinee_temp(option_ty, span);
            layout.enter_scope();
            (iter_temp_slot, option_match_temp)
        } else {
            return;
        };
        self.define_local(
            binding.symbol,
            item_ty,
            BindingKind::Var,
            None,
            binding.span,
        );
        self.with_loop_body(body, |this| this.check_block(body));
        if let Some(layout) = &mut self.layout {
            layout.exit_scope();
            layout.plan_for_in(ForInPlan {
                binding: binding.symbol,
                item_ty,
                iter_temp_slot,
                iter_state_ty,
                into_iter_fn,
                next_fn,
                option_ty,
                option_match_temp,
                some_variant,
                stmt_span: span,
            });
        }
    }
}

fn drop_prim_kind_byte(types: &TypeInterner, ty: TypeId) -> u8 {
    if matches!(types.get(ty), Ty::Fn { .. }) {
        SLOT_KIND_FN_PTR
    } else {
        primitive_kind_for_type(types, ty).map_or(SLOT_KIND_AGG, phx_bytecode::PrimitiveKind::as_u8)
    }
}

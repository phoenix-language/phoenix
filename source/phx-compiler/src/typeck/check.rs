//! Type-checking driver and AST walk.

use std::collections::{HashMap, HashSet};

use phx_diagnostics::{MismatchKind, Span, TypeCheckBag, TypeCheckError};
use phx_syntax::ast::decl::{
    Function, ImplMember, Param, StructBody, TopLevelDecl, TopLevelItem, TraitItem, Variant,
};
use phx_syntax::ast::expr::{Expr, IfCondition, LambdaBody, PostfixOp, StructFieldInit, UnaryOp};
use phx_syntax::ast::ident::{Ident, Path, PathSegment, TypeName};
use phx_syntax::ast::lit::Literal;
use phx_syntax::ast::pat::Pattern;
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};
use phx_syntax::ast::types::GenericParam;
use phx_syntax::ast::types::Type;
use phx_syntax::ast::{BlockNode, ExprNode, Node};
use phx_syntax::{Symbol, for_in_iter_symbol, impl_receiver_symbol};

use super::IndirectCallMeta;
use super::PrimitiveMethodSite;
use super::bindings::{BindingKind, DropEvent, ForInPlan, FunctionLayout, FunctionLayoutBuilder};
use super::bounds::{
    resolve_from_fn_for_error, trait_bound_head, type_satisfies_trait_inst,
    validate_instantiation_bounds,
};
use super::builtins::{
    bool_type, float_literal_type, implements_drop, implements_drop_for_def, int_literal_type,
    is_copyable, resolve_drop_fn, str_type, u8_type, unit,
};
use super::display::{format_type, format_type_diagnostic};
use super::infer::InferenceCtx;
use super::intrinsic_kernel::{IntrinsicKernel, IntrinsicSite};
use super::layout::{
    EnumLayout, ProgramLayout, StructLayout, TraitInstKey, TypeMonoKey, VariantKind, VariantLayout,
    VariantMeta,
};
use super::lower_ty::{TypeDefMap, build_type_def_map, error_type, lower_type, push_generics};
use super::mono::{
    MonoInst, TypeMonoInst, TypeMonoKind, generic_param_defs_for_type, generic_params_for_def,
};
use super::ops::{check_binary, check_cast, check_unary};
use super::ownership::OwnershipTracker;
use super::primitive::{is_int_keyword, primitive_kind_for_type};
use super::std_kernel::{StdKernel, TryFailureMode, TrySiteMeta};
use super::std_trait_kernel::StdTraitKernel;
use super::subst::Substitution;
use super::trait_defaults;
use super::types::is_error_type;
use super::types::{ExprId, Ty, TypeId, TypeInterner};
use super::unify::AliasEnv;
use super::unify::unify_branch;
use crate::resolver::{DefId, DefKind, ResolutionKey, ResolvedProgram};
use phx_bytecode::{SLOT_KIND_AGG, SLOT_KIND_FN_PTR};
use phx_syntax::token::Keyword;

/// Collected struct field types.
#[derive(Debug, Clone)]
pub struct StructFields {
    /// Field name → type.
    pub fields: HashMap<Symbol, TypeId>,
}

/// Type checker state for one compilation unit.
///
/// Walks the resolved AST, fills [`TypeInterner`] and per-expression [`TypeId`]s, and records
/// errors in a [`TypeCheckBag`] without stopping at the first failure.
pub struct TypeChecker<'a> {
    resolved: &'a ResolvedProgram,
    types: TypeInterner,
    bag: TypeCheckBag,
    expr_types: HashMap<ExprId, TypeId>,
    /// Expression types keyed by `(module_id, source span)` for lint discard checks.
    expr_span_types: HashMap<(u32, Span), TypeId>,
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
    /// Layout scope depth at each active loop body entry (before `check_block` `enter_scope`).
    loop_body_scope_depths: Vec<u32>,
    /// Struct/enum layouts for lowering and codegen.
    program_layout: ProgramLayout,
    next_type_id: u32,
    /// When checking inherent impl members, the receiver type (`Self`).
    impl_self_type: Option<TypeId>,
    /// When checking a trait impl, `(implementer type def, trait def)`.
    active_trait_impl: Option<(DefId, DefId)>,
    /// Associated types assigned on the active trait impl (`Item` → concrete type).
    impl_assoc_types: HashMap<Symbol, TypeId>,
    /// Abstract associated types while collecting a trait definition.
    trait_assoc_abstract: HashMap<Symbol, TypeId>,
    /// Module being collected or checked.
    current_module: u32,
    /// Active type substitution when checking a monomorphized clone.
    subst: Option<Substitution>,
    /// Explicit generic instantiations to specialize after the main pass.
    mono_insts: Vec<MonoInst>,
    /// Explicit generic type instantiations to specialize after the main pass.
    type_mono_insts: Vec<TypeMonoInst>,
    /// Expanded alias types keyed by monomorphization key (filled during checking).
    specialized_aliases: HashMap<TypeMonoKey, TypeId>,
    /// Std `Option` / `Result` ids for `?` sugar.
    std_kernel: StdKernel,
    /// Std core trait ids for bound checking.
    std_trait_kernel: StdTraitKernel,
    /// `expr?` sites for lowering.
    try_sites: HashMap<ExprId, TrySiteMeta>,
    /// Primitive trait method sites for lowering.
    primitive_method_sites: HashMap<ExprId, PrimitiveMethodSite>,
    /// Trait associated fn call sites (`Target::from`) → monomorphized or template fn def.
    associated_fn_sites: HashMap<ExprId, DefId>,
    /// Method call sites (`recv.method`) → monomorphized or template fn def.
    method_call_sites: HashMap<ExprId, DefId>,
    /// Indirect fn pointer call sites for lowering.
    indirect_call_sites: HashMap<ExprId, IndirectCallMeta>,
    /// VM intrinsic call sites (`alloc_bytes`, …).
    intrinsic_call_sites: HashMap<ExprId, IntrinsicSite>,
    /// Compile-time `size_of` results keyed by postfix `Call` expression id.
    size_of_literals: HashMap<ExprId, u32>,
    /// Kernel of std intrinsic definition ids.
    intrinsic_kernel: IntrinsicKernel,
    /// Nesting depth of `unsafe` blocks and `unsafe fn` bodies.
    unsafe_depth: u32,
    /// Pending synthetic fn defs for inherited trait defaults (merged into resolved at finish).
    pending_inherited_defs: Vec<crate::resolver::Def>,
    /// Inherited trait default bodies keyed by synthetic `DefId`.
    inherited_trait_methods: trait_defaults::InheritedTraitMethods,
    /// Inherited methods grouped by trait impl key (for body type checking).
    inherited_by_inst: trait_defaults::InheritedByInst,
    /// Trait defs marked `unsafe trait`.
    trait_unsafe: HashMap<DefId, bool>,
    /// Fn defs that require `unsafe` at call sites.
    fn_effective_unsafe: HashMap<DefId, bool>,
}

impl<'a> TypeChecker<'a> {
    fn new(resolved: &'a ResolvedProgram) -> Self {
        Self::new_with_types(resolved, TypeInterner::new())
    }

    /// Type-checker seeded with a substitution map for one monomorphized function body.
    pub(crate) fn new_with_substitution(
        resolved: &'a ResolvedProgram,
        subst: Substitution,
        types: TypeInterner,
    ) -> Self {
        let mut checker = Self::new_with_types(resolved, types);
        checker.subst = Some(subst);
        checker
    }

    fn new_with_types(resolved: &'a ResolvedProgram, mut types: TypeInterner) -> Self {
        let unit = unit(&mut types);
        let bool_ty = bool_type(&mut types);
        let named_paths = crate::pxi::build_named_def_paths(resolved);
        let mut value_types = HashMap::new();
        for (def_id, pxi_ty) in &resolved.import_types {
            let Some(def) = resolved.defs.get(def_id.index() as usize) else {
                continue;
            };
            let mut ctx = crate::pxi::PxiImportCtx {
                types: &mut types,
                interner: &resolved.interner,
                named_defs: &named_paths,
            };
            crate::pxi::seed_value_type(&mut ctx, *def_id, def.kind, pxi_ty, &mut value_types);
        }
        Self {
            resolved,
            types,
            bag: TypeCheckBag::new(),
            expr_types: HashMap::new(),
            expr_span_types: HashMap::new(),
            next_expr: 0,
            type_defs: build_type_def_map(&resolved.defs),
            value_types,
            struct_fields: HashMap::new(),
            fn_ret: None,
            ownership: OwnershipTracker::new(),
            unit,
            bool_ty,
            functions: Vec::new(),
            layout: None,
            ctor_expected: None,
            loop_depth: 0,
            loop_body_scope_depths: Vec::new(),
            program_layout: ProgramLayout::default(),
            next_type_id: 1,
            impl_self_type: None,
            active_trait_impl: None,
            impl_assoc_types: HashMap::new(),
            trait_assoc_abstract: HashMap::new(),
            current_module: resolved.root,
            subst: None,
            mono_insts: Vec::new(),
            type_mono_insts: Vec::new(),
            specialized_aliases: HashMap::new(),
            std_kernel: StdKernel::default(),
            std_trait_kernel: StdTraitKernel::default(),
            try_sites: HashMap::new(),
            primitive_method_sites: HashMap::new(),
            associated_fn_sites: HashMap::new(),
            method_call_sites: HashMap::new(),
            indirect_call_sites: HashMap::new(),
            intrinsic_call_sites: HashMap::new(),
            size_of_literals: HashMap::new(),
            intrinsic_kernel: IntrinsicKernel::default(),
            unsafe_depth: 0,
            pending_inherited_defs: Vec::new(),
            inherited_trait_methods: HashMap::new(),
            inherited_by_inst: HashMap::new(),
            trait_unsafe: HashMap::new(),
            fn_effective_unsafe: HashMap::new(),
        }
    }

    pub(crate) fn seed_fn_effective_unsafe(&mut self, map: &HashMap<DefId, bool>) {
        self.fn_effective_unsafe
            .extend(map.iter().map(|(k, v)| (*k, *v)));
    }

    fn is_effective_unsafe(&self, def: DefId) -> bool {
        self.fn_effective_unsafe.get(&def).copied().unwrap_or(false)
    }

    fn mark_fn_effective_unsafe(&mut self, def: DefId) {
        self.fn_effective_unsafe.insert(def, true);
    }

    fn check_unsafe_fn_call(&mut self, def: DefId, span: Span) {
        if !self.is_effective_unsafe(def) || self.unsafe_depth > 0 {
            return;
        }
        let Some(record) = self.resolved.defs.get(def.index() as usize) else {
            return;
        };
        self.bag.push(
            self.current_module,
            TypeCheckError::UnsafeFnCallRequiresUnsafe {
                name: self.symbol_name(record.name),
                span,
            },
        );
    }

    fn trait_method_sig_unsafe(
        sig: &phx_syntax::ast::decl::FunctionSig,
        trait_unsafe: bool,
    ) -> bool {
        trait_unsafe || sig.unsafe_
    }

    fn validate_impl_unsafe(
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

    fn is_copyable_ty(&self, ty: TypeId) -> bool {
        if implements_drop(
            &self.types,
            &self.program_layout,
            self.resolved,
            &self.std_trait_kernel,
            ty,
        ) {
            return false;
        }
        is_copyable(
            &self.types,
            &self.program_layout,
            &self.std_trait_kernel,
            ty,
        )
    }

    pub(crate) fn take_mono_insts(&mut self) -> Vec<MonoInst> {
        std::mem::take(&mut self.mono_insts)
    }

    pub(crate) fn take_type_mono_insts(&mut self) -> Vec<TypeMonoInst> {
        std::mem::take(&mut self.type_mono_insts)
    }

    pub(crate) fn set_expr_id_base(&mut self, base: u32) {
        self.next_expr = base;
    }

    /// Copies collected layout tables from the template type-check pass for mono re-checks.
    pub(crate) fn seed_layout_tables(&mut self, layout: &ProgramLayout) {
        self.program_layout = layout.clone();
        for (def, struct_layout) in &layout.structs {
            let fields: HashMap<Symbol, TypeId> = struct_layout.fields.iter().copied().collect();
            self.struct_fields.insert(*def, StructFields { fields });
        }
    }

    /// Reuses std kernel metadata from the template type-check pass during monomorphization.
    pub(crate) fn seed_std_kernel(&mut self, kernel: &StdKernel) {
        self.std_kernel = kernel.clone();
    }

    /// Reuses std trait kernel from the template pass during monomorphization.
    pub(crate) fn seed_intrinsic_kernel(&mut self, kernel: &IntrinsicKernel) {
        self.intrinsic_kernel = kernel.clone();
    }

    pub(crate) fn seed_std_trait_kernel(&mut self, kernel: &StdTraitKernel) {
        self.std_trait_kernel = kernel.clone();
    }

    /// Reuses template-pass value types (fn sigs, struct types, …) during mono body re-checks.
    pub(crate) fn seed_value_types(&mut self, value_types: &HashMap<DefId, TypeId>) {
        self.value_types
            .extend(value_types.iter().map(|(k, v)| (*k, *v)));
    }

    pub(crate) fn check_function_specialized(
        &mut self,
        f: &Function,
        spec_def: DefId,
        base_fn: DefId,
        mono_args: &[TypeId],
    ) {
        let fn_ty = self.fn_type_for_function(f);
        let fn_ty = if let Some(subst) = &self.subst {
            match self.types.get(fn_ty).clone() {
                Ty::Fn { params, ret } => {
                    let params: Vec<_> = params
                        .iter()
                        .map(|p| Substitution::apply(&mut self.types, *p, subst))
                        .collect();
                    let ret = Substitution::apply(&mut self.types, ret, subst);
                    self.types.intern(&Ty::Fn { params, ret })
                }
                other => self.types.intern(&other),
            }
        } else {
            fn_ty
        };
        self.value_types.insert(spec_def, fn_ty);
        if let Ty::Fn { ret, .. } = self.types.get(fn_ty).clone() {
            self.fn_ret = Some(ret);
        }
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
        self.check_function_body(f, spec_def, true, true);
        self.impl_self_type = saved_impl_self;
        self.current_module = saved_module;
        self.fn_ret = None;
    }

    fn impl_type_for_method(&self, fn_def: DefId) -> Option<DefId> {
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

    fn def_module(&self, def: DefId) -> u32 {
        self.resolved
            .defs
            .get(def.index() as usize)
            .map(|d| d.module)
            .unwrap_or(self.current_module)
    }

    fn fn_module(&self, f: &Function) -> u32 {
        self.fn_def_for(f)
            .map(|d| self.def_module(d))
            .unwrap_or(self.current_module)
    }

    /// Temporarily extends `type_defs` with generic parameters for `module`.
    fn with_pushed_generics<R>(
        &mut self,
        module: u32,
        generics: Option<&[phx_syntax::ast::types::GenericParam]>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let saved = self.type_defs.clone();
        push_generics(&mut self.type_defs, &self.resolved.defs, module, generics);
        let result = f(self);
        self.type_defs = saved;
        result
    }

    fn fn_type_for_function(&mut self, f: &Function) -> TypeId {
        let module = self.fn_module(f);
        let generics = f.generics.as_deref();
        self.with_pushed_generics(module, generics, |this| {
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

    fn alloc_type_id(&mut self, def: DefId) -> u32 {
        let id = self.next_type_id;
        self.next_type_id += 1;
        self.program_layout.type_ids.insert(def, id);
        id
    }

    fn error_loop_control_outside_loop(&mut self, keyword: &'static str, span: Span) {
        self.bag.push(
            self.current_module,
            TypeCheckError::LoopControlOutsideLoop { keyword, span },
        );
    }

    fn with_loop_body<F: FnOnce(&mut Self)>(&mut self, body: &Block, f: F) {
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

    fn check_loop_back_edge_uses(
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

    fn collect_ident_read_uses_in_block(&self, block: &Block, symbol: Symbol, out: &mut Vec<Span>) {
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

    fn collect_ident_read_uses_in_stmt(&self, stmt: &Stmt, symbol: Symbol, out: &mut Vec<Span>) {
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

    fn expr_is_move_source_ident(expr: &ExprNode, symbol: Symbol) -> bool {
        matches!(&expr.inner, Expr::Ident(ident) if ident.symbol == symbol)
    }

    fn collect_ident_read_uses_in_expr(&self, expr: &Expr, symbol: Symbol, out: &mut Vec<Span>) {
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

    fn collect_ident_read_uses_in_postfix(
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

    fn collect_ident_read_uses_in_if_expr(
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

    fn collect_ident_read_uses_in_if_condition(
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

    fn collect_ident_read_uses_in_assign_target(
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

    fn layout_scope_depth(&self) -> u32 {
        self.layout
            .as_ref()
            .map_or(0, FunctionLayoutBuilder::scope_depth)
    }

    fn plan_drops_for_scope_depths(&mut self, from_depth: u32, to_depth: u32) {
        if from_depth < to_depth {
            return;
        }
        for depth in (to_depth..=from_depth).rev() {
            self.plan_drops_at_scope_depth(depth);
        }
    }

    fn plan_drops_at_scope_depth(&mut self, depth: u32) {
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
            });
        }
        if let Some(layout) = &mut self.layout {
            for event in planned {
                layout.plan_drop(event);
            }
        }
    }

    fn mark_method_receiver_moved(
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

    fn method_consumes_receiver(&self, fn_def: DefId, receiver_ty: TypeId) -> bool {
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

    fn fn_is_drop_method(&self, fn_def: DefId) -> bool {
        self.program_layout
            .trait_methods
            .iter()
            .any(|((key, method), &def)| {
                def == fn_def
                    && self.std_trait_kernel.is_drop_trait(key.trait_def)
                    && self.resolved.interner.resolves_to(*method, "drop")
            })
    }

    fn check_copyable_drop_conflict(&mut self, type_def: DefId, trait_def: DefId, span: Span) {
        let conflicts = if self.std_trait_kernel.is_copyable_trait(trait_def) {
            implements_drop_for_def(
                &self.program_layout,
                self.resolved,
                &self.std_trait_kernel,
                type_def,
                &[],
            )
        } else if self.std_trait_kernel.is_drop_trait(trait_def) {
            self.program_layout.trait_impls.iter().any(|key| {
                key.implementer == type_def
                    && self.std_trait_kernel.is_copyable_trait(key.trait_def)
            }) || self.program_layout.trait_methods.keys().any(|(key, _)| {
                key.implementer == type_def
                    && self.std_trait_kernel.is_copyable_trait(key.trait_def)
            })
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

    fn with_unsafe<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.unsafe_depth = self.unsafe_depth.saturating_add(1);
        f(self);
        self.unsafe_depth = self.unsafe_depth.saturating_sub(1);
    }

    fn alloc_fn_sig_type_id(&mut self, fn_ty: TypeId) -> u32 {
        if let Some(&id) = self.program_layout.fn_sig_ids.get(&fn_ty) {
            return id;
        }
        let Ty::Fn { params, .. } = self.types.get(fn_ty).clone() else {
            return 0;
        };
        let id = self.next_type_id;
        self.next_type_id = self.next_type_id.saturating_add(1);
        self.program_layout.fn_sig_ids.insert(fn_ty, id);
        let _ = params;
        id
    }

    fn is_static_fn_callee(&self, def: DefId) -> bool {
        self.resolved
            .defs
            .get(def.index() as usize)
            .is_some_and(|d| d.kind == DefKind::Fn)
    }

    fn check_extern_call(&mut self, def: DefId, span: Span) {
        let Some(record) = self.resolved.defs.get(def.index() as usize) else {
            return;
        };
        if record.kind != DefKind::ExternFn || self.unsafe_depth > 0 {
            return;
        }
        self.bag.push(
            self.current_module,
            TypeCheckError::ExternCallRequiresUnsafe {
                name: self.symbol_name(record.name),
                span,
            },
        );
    }

    fn check_alloc_bytes_args(&mut self, args: &[ExprNode], span: Span) {
        let u32_ty = super::builtins::int_literal_type(&mut self.types, true);
        if args.len() == 1 {
            let got = self.check_expr_node(&args[0]);
            if !self.types_equal(got, u32_ty) {
                self.error_mismatch(
                    u32_ty,
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
    }

    fn check_dealloc_bytes_args(&mut self, args: &[ExprNode], span: Span) {
        let u32_ty = super::builtins::int_literal_type(&mut self.types, true);
        let u8_ty = super::builtins::u8_type(&mut self.types);
        let ptr_ty = self.types.intern(&Ty::Ptr {
            mut_: true,
            inner: u8_ty,
        });
        if args.len() == 2 {
            let got_ptr = self.check_expr_node(&args[0]);
            if !self.types_equal(got_ptr, ptr_ty) {
                self.error_mismatch(
                    ptr_ty,
                    got_ptr,
                    args[0].span,
                    MismatchKind::Argument { index: 0 },
                );
            }
            let got_size = self.check_expr_node(&args[1]);
            if !self.types_equal(got_size, u32_ty) {
                self.error_mismatch(
                    u32_ty,
                    got_size,
                    args[1].span,
                    MismatchKind::Argument { index: 1 },
                );
            }
        } else {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: 2,
                    found: args.len(),
                    span,
                },
            );
        }
    }

    #[allow(clippy::too_many_lines)]
    fn check_intrinsic_call(
        &mut self,
        site: IntrinsicSite,
        def: DefId,
        type_args: Option<&[Node<Type>]>,
        args: &[ExprNode],
        span: Span,
        expr_id: ExprId,
    ) -> TypeId {
        if site != IntrinsicSite::SizeOf && self.unsafe_depth == 0 {
            let name = self
                .resolved
                .defs
                .get(def.index() as usize)
                .map(|d| self.symbol_name(d.name))
                .unwrap_or_else(|| "intrinsic".to_owned());
            self.bag.push(
                self.current_module,
                TypeCheckError::IntrinsicRequiresUnsafe { name, span },
            );
        }
        match site {
            IntrinsicSite::AllocBytes => {
                let u8_ty = super::builtins::u8_type(&mut self.types);
                let ret = self.types.intern(&Ty::Ptr {
                    mut_: true,
                    inner: u8_ty,
                });
                self.check_alloc_bytes_args(args, span);
                self.intrinsic_call_sites.insert(expr_id, site);
                ret
            }
            IntrinsicSite::DeallocBytes => {
                self.check_dealloc_bytes_args(args, span);
                self.intrinsic_call_sites.insert(expr_id, site);
                self.unit
            }
            IntrinsicSite::SliceFromRawParts => {
                let u32_ty = super::builtins::int_literal_type(&mut self.types, true);
                let unit_ret = self.unit;
                if args.len() != 2 {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::ArityMismatch {
                            expected: 2,
                            found: args.len(),
                            span,
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return unit_ret;
                }
                let ptr_ty = self.check_expr_node(&args[0]);
                let len_ty = self.check_expr_node(&args[1]);
                if !self.types_equal(len_ty, u32_ty) {
                    self.error_mismatch(
                        u32_ty,
                        len_ty,
                        args[1].span,
                        MismatchKind::Argument { index: 1 },
                    );
                }
                let Ty::Ptr { inner, .. } = self.types.get(ptr_ty).clone() else {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::Mismatch {
                            expected: "*mut T".to_owned(),
                            found: self.format_ty_diagnostic(ptr_ty),
                            span: args[0].span,
                            kind: MismatchKind::Argument { index: 0 },
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return unit_ret;
                };
                if super::primitive_kind_for_type(&self.types, inner).is_none() {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnsupportedFeature {
                            feature: "heap slice over non-primitive element type",
                            span,
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return unit_ret;
                }
                let ret = self.types.intern(&Ty::Slice(inner));
                self.intrinsic_call_sites.insert(expr_id, site);
                ret
            }
            IntrinsicSite::SizeOf => {
                let u32_ty = super::builtins::int_literal_type(&mut self.types, true);
                if !args.is_empty() {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::ArityMismatch {
                            expected: 0,
                            found: args.len(),
                            span,
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return u32_ty;
                }
                let Some(type_args) = type_args else {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnsupportedFeature {
                            feature: "size_of requires an explicit type argument",
                            span,
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return u32_ty;
                };
                if type_args.len() != 1 {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnsupportedFeature {
                            feature: "size_of expects exactly one type argument",
                            span,
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return u32_ty;
                }
                let type_defs = self.type_defs.clone();
                let mut queried = self.lower_ast_type_with_defs(&type_args[0], &type_defs);
                if let Some(subst) = &self.subst {
                    queried = Substitution::apply(&mut self.types, queried, subst);
                }
                let Some(bytes) = super::type_size::type_byte_size(
                    &self.types,
                    &self.program_layout,
                    self.resolved,
                    queried,
                ) else {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnsupportedFeature {
                            feature: "size_of for this type",
                            span: type_args[0].span,
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return u32_ty;
                };
                self.size_of_literals.insert(expr_id, bytes);
                self.intrinsic_call_sites.insert(expr_id, site);
                u32_ty
            }
        }
    }

    fn record_indirect_call(
        &mut self,
        callee_ty: TypeId,
        callee_def: Option<DefId>,
        expr_id: ExprId,
        foreign: bool,
    ) {
        if callee_def.is_some_and(|d| self.is_static_fn_callee(d)) {
            return;
        }
        let Ty::Fn { params, .. } = self.types.get(callee_ty).clone() else {
            return;
        };
        let sig_type_id = self.alloc_fn_sig_type_id(callee_ty);
        let expected_arity = u32::try_from(params.len()).unwrap_or(u32::MAX);
        self.indirect_call_sites.insert(
            expr_id,
            IndirectCallMeta {
                sig_type_id,
                expected_arity,
                foreign,
            },
        );
    }

    fn check_with_ownership_fork<R>(
        &mut self,
        pre: &OwnershipTracker,
        f: impl FnOnce(&mut Self) -> R,
    ) -> (R, OwnershipTracker) {
        self.ownership = pre.clone();
        let result = f(self);
        let end = self.ownership.clone();
        (result, end)
    }

    fn enter_scope(&mut self) {
        self.ownership.enter_scope();
        if let Some(layout) = &mut self.layout {
            layout.enter_scope();
        }
    }

    fn exit_scope(&mut self) {
        let exiting = self.layout_scope_depth();
        self.plan_drops_at_scope_depth(exiting);
        self.ownership.exit_scope();
        if let Some(layout) = &mut self.layout {
            layout.exit_scope();
        }
    }

    fn fn_def_for(&self, f: &Function) -> Option<DefId> {
        self.find_def(self.current_module, f.name.symbol, DefKind::Fn)
    }

    fn alias_env(&self) -> AliasEnv<'_> {
        AliasEnv {
            types: &self.types,
            defs: &self.resolved.defs,
            value_types: &self.value_types,
        }
    }

    fn types_equal(&self, a: TypeId, b: TypeId) -> bool {
        super::unify::same_type(&self.alias_env(), a, b)
    }

    fn utf8_rodata_for_const_init(init: &Expr) -> Option<Vec<u8>> {
        if let Expr::Literal(Literal::ByteString(b)) = init {
            if std::str::from_utf8(b).is_ok() {
                return Some(b.clone());
            }
        }
        None
    }

    fn define_local(
        &mut self,
        symbol: phx_syntax::Symbol,
        ty: TypeId,
        kind: BindingKind,
        init: Option<&Expr>,
    ) {
        self.ownership.define(symbol, ty);
        if let Some(layout) = &mut self.layout {
            let utf8_rodata = if kind == BindingKind::Const {
                init.and_then(Self::utf8_rodata_for_const_init)
            } else {
                None
            };
            let _ = layout.alloc(symbol, ty, kind, utf8_rodata);
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

    fn format_ty_diagnostic(&self, id: TypeId) -> String {
        format_type_diagnostic(
            &self.types,
            &self.resolved.interner,
            &self.resolved.defs,
            id,
        )
    }

    fn error_mismatch(&mut self, expected: TypeId, found: TypeId, span: Span, kind: MismatchKind) {
        self.bag.push(
            self.current_module,
            TypeCheckError::Mismatch {
                expected: self.format_ty_diagnostic(expected),
                found: self.format_ty_diagnostic(found),
                span,
                kind,
            },
        );
    }

    fn poison_type(&mut self) -> TypeId {
        error_type(&mut self.types)
    }

    fn impl_self_type_id(
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

    fn is_self_type_name(&self, symbol: Symbol) -> bool {
        self.resolved.interner.resolves_to(symbol, "Self")
    }

    fn lower_ast_type_with_defs(&mut self, ty: &Node<Type>, type_defs: &TypeDefMap) -> TypeId {
        if let Type::SelfAssoc { member } = &ty.inner {
            if let Some(&concrete) = self.impl_assoc_types.get(&member.symbol) {
                return concrete;
            }
            if let Some(&abstract_ty) = self.trait_assoc_abstract.get(&member.symbol) {
                return abstract_ty;
            }
            self.bag.push(
                self.current_module,
                TypeCheckError::UnknownType {
                    symbol_index: member.symbol.index(),
                    span: ty.span,
                },
            );
            return self.poison_type();
        }
        if let Type::Named { name, generics } = &ty.inner {
            if self.is_self_type_name(name.symbol) {
                if let Some(self_ty) = self.impl_self_type {
                    if let Some(gs) = generics {
                        if !gs.is_empty() {
                            self.bag.push(
                                self.current_module,
                                TypeCheckError::UnsupportedFeature {
                                    feature: "generic arguments on Self",
                                    span: ty.span,
                                },
                            );
                        }
                    }
                    return self_ty;
                }
            }
        }
        let id = match &ty.inner {
            Type::Ref { mut_, inner } => {
                let i = self.lower_ast_type_with_defs(inner, type_defs);
                self.types.intern(&Ty::Ref {
                    mut_: *mut_,
                    inner: i,
                })
            }
            Type::Ptr { mut_, inner } => {
                let i = self.lower_ast_type_with_defs(inner, type_defs);
                self.types.intern(&Ty::Ptr {
                    mut_: *mut_,
                    inner: i,
                })
            }
            Type::Function { params, ret } => {
                let ps: Vec<_> = params
                    .iter()
                    .map(|p| self.lower_ast_type_with_defs(p, type_defs))
                    .collect();
                let r = self.lower_ast_type_with_defs(ret, type_defs);
                self.types.intern(&Ty::Fn { params: ps, ret: r })
            }
            Type::Tuple(ts) => {
                let elems: Vec<_> = ts
                    .iter()
                    .map(|t| self.lower_ast_type_with_defs(t, type_defs))
                    .collect();
                self.types.intern(&Ty::Tuple(elems))
            }
            Type::Array { elem, len } => {
                let e = self.lower_ast_type_with_defs(elem, type_defs);
                let length = u32::try_from(len.value).unwrap_or(0);
                self.types.intern(&Ty::Array {
                    elem: e,
                    len: length,
                })
            }
            Type::Slice(inner) => {
                let i = self.lower_ast_type_with_defs(inner, type_defs);
                self.types.intern(&Ty::Slice(i))
            }
            _ => lower_type(&mut self.types, type_defs, &ty.inner),
        };
        let id = if let Some(subst) = &self.subst {
            Substitution::apply(&mut self.types, id, subst)
        } else {
            id
        };
        if let Ty::Named { def, args } = self.types.get(id).clone() {
            if !args.is_empty() {
                return self.resolve_instantiated_named(def, args, ty.span);
            }
        }
        if is_error_type(&self.types, id) {
            if let Type::Named { name, .. } = &ty.inner {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::UnknownType {
                        symbol_index: name.symbol.index(),
                        span: ty.span,
                    },
                );
            }
        }
        id
    }

    fn validate_type_aliases(&mut self) {
        for (index, def) in self.resolved.defs.iter().enumerate() {
            if def.kind != DefKind::TypeAlias {
                continue;
            }
            let alias_id = DefId::from_raw(u32::try_from(index).unwrap_or(u32::MAX));
            let Some(&start) = self.value_types.get(&alias_id) else {
                continue;
            };
            let mut stack = std::collections::HashSet::new();
            if self.type_alias_cycle_from(start, &mut stack) {
                self.bag.push(
                    def.module,
                    TypeCheckError::RecursiveTypeAlias { span: def.span },
                );
            }
        }
    }

    fn type_alias_cycle_from(
        &self,
        current: TypeId,
        stack: &mut std::collections::HashSet<DefId>,
    ) -> bool {
        let Ty::Named { def, .. } = self.types.get(current) else {
            return false;
        };
        if !stack.insert(*def) {
            return true;
        }
        let Some(def_record) = self.resolved.defs.get(def.index() as usize) else {
            stack.remove(def);
            return false;
        };
        if def_record.kind != DefKind::TypeAlias {
            stack.remove(def);
            return false;
        }
        let Some(&next) = self.value_types.get(def) else {
            stack.remove(def);
            return false;
        };
        let cycled = self.type_alias_cycle_from(next, stack);
        stack.remove(def);
        cycled
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

    fn lookup_resolution(&self, node_id: phx_syntax::AstNodeId) -> Option<DefId> {
        self.resolved
            .resolutions
            .get(&ResolutionKey {
                module: self.current_module,
                node_id,
            })
            .copied()
    }

    fn check_program(&mut self) {
        self.collect_decls();
        self.std_kernel = StdKernel::build(self.resolved, &self.program_layout);
        self.std_trait_kernel = StdTraitKernel::build(self.resolved, &self.program_layout);
        self.intrinsic_kernel = IntrinsicKernel::build(self.resolved);
        self.validate_type_aliases();
        for module in &self.resolved.modules {
            self.current_module = module.id;
            for item in &module.program.items {
                self.check_decl_derives(&item.inner, item.span);
                self.check_top_level(&item.inner, item.span);
            }
        }
    }

    fn check_decl_derives(&mut self, item: &TopLevelItem, span: Span) {
        let derives = match &item.decl {
            TopLevelDecl::Struct { derives, .. }
            | TopLevelDecl::Enum { derives, .. }
            | TopLevelDecl::Trait { derives, .. } => derives,
            TopLevelDecl::Function(f) => &f.derives,
            TopLevelDecl::Impl { members, .. } => {
                for m in members {
                    if let ImplMember::Method(f) = m {
                        if !f.derives.is_empty() {
                            self.push_unsupported("#derive on impl method", f.body.span);
                        }
                    }
                }
                return;
            }
            TopLevelDecl::Const { .. }
            | TopLevelDecl::Var { .. }
            | TopLevelDecl::TypeAlias { .. } => {
                return;
            }
            _ => return,
        };
        if !derives.is_empty() {
            let _ = span;
        }
    }

    fn push_unsupported(&mut self, feature: &'static str, span: Span) {
        self.bag.push(
            self.current_module,
            TypeCheckError::UnsupportedFeature { feature, span },
        );
    }

    fn push_internal_error(&mut self, detail: &'static str, span: Span) {
        self.bag.push(
            self.current_module,
            TypeCheckError::InternalError { detail, span },
        );
    }

    fn collect_decls(&mut self) {
        for module in &self.resolved.modules {
            self.current_module = module.id;
            for item in &module.program.items {
                self.collect_top_level_decl(&item.inner.decl);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn collect_top_level_decl(&mut self, decl: &TopLevelDecl) {
        match decl {
            TopLevelDecl::Struct {
                name,
                derives,
                generics,
                body,
            } => {
                let _ = derives;
                self.with_pushed_generics(self.current_module, generics.as_deref(), |this| {
                    let type_defs = this.type_defs.clone();
                    if let Some(def) =
                        this.find_def(this.current_module, name.symbol, DefKind::Struct)
                    {
                        let mut fields_map = HashMap::new();
                        let mut ordered = Vec::new();
                        match body {
                            StructBody::Fields(fs) => {
                                for f in fs {
                                    let ty = this.lower_ast_type_with_defs(&f.ty, &type_defs);
                                    fields_map.insert(f.name.symbol, ty);
                                    ordered.push((f.name.symbol, ty));
                                }
                            }
                            StructBody::Tuple(types) => {
                                this.program_layout.tuple_structs.insert(def);
                                for (i, t) in types.iter().enumerate() {
                                    let ty = this.lower_ast_type_with_defs(t, &type_defs);
                                    let Some(field_name) = this.tuple_field_symbol(i) else {
                                        continue;
                                    };
                                    fields_map.insert(field_name, ty);
                                    ordered.push((field_name, ty));
                                }
                            }
                            StructBody::Unit => {}
                        }
                        this.struct_fields
                            .insert(def, StructFields { fields: fields_map });
                        this.program_layout
                            .structs
                            .insert(def, StructLayout { fields: ordered });
                        let _ = this.alloc_type_id(def);
                        let struct_ty = this.types.intern(&Ty::Named { def, args: vec![] });
                        this.value_types.insert(def, struct_ty);
                    }
                });
            }
            TopLevelDecl::Enum {
                name,
                derives,
                generics,
                variants,
            } => {
                let _ = derives;
                self.with_pushed_generics(self.current_module, generics.as_deref(), |this| {
                    let type_defs = this.type_defs.clone();
                    if let Some(enum_def) =
                        this.find_def(this.current_module, name.symbol, DefKind::Enum)
                    {
                        let enum_ty = this.types.intern(&Ty::Named {
                            def: enum_def,
                            args: vec![],
                        });
                        this.value_types.insert(enum_def, enum_ty);
                        let type_id = this.alloc_type_id(enum_def);
                        let _ = type_id;
                        let mut variant_layouts = Vec::new();
                        for (tag, v) in variants.iter().enumerate() {
                            let tag = u32::try_from(tag).unwrap_or(u32::MAX);
                            let variant_def = this.find_def(
                                this.current_module,
                                v.name.symbol,
                                DefKind::EnumVariant,
                            );
                            let (payload_types, kind) = match &v.kind {
                                Variant::Unit => (vec![], VariantKind::Unit),
                                Variant::Tuple(ts) => {
                                    let pts: Vec<TypeId> = ts
                                        .iter()
                                        .map(|t| this.lower_ast_type_with_defs(t, &type_defs))
                                        .collect();
                                    (pts.clone(), VariantKind::Tuple(pts))
                                }
                                Variant::Struct(fs) => {
                                    let fields: Vec<(Symbol, TypeId)> = fs
                                        .iter()
                                        .map(|f| {
                                            (
                                                f.name.symbol,
                                                this.lower_ast_type_with_defs(&f.ty, &type_defs),
                                            )
                                        })
                                        .collect();
                                    let pts: Vec<TypeId> =
                                        fields.iter().map(|(_, ty)| *ty).collect();
                                    (pts, VariantKind::Struct(fields))
                                }
                            };
                            if let Some(vdef) = variant_def {
                                let params: Vec<TypeId> = payload_types.clone();
                                let ctor_ty = this.types.intern(&Ty::Fn {
                                    params,
                                    ret: enum_ty,
                                });
                                this.value_types.insert(vdef, ctor_ty);
                                this.program_layout.variants.insert(
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
                        this.program_layout.enums.insert(
                            enum_def,
                            EnumLayout {
                                enum_def,
                                variants: variant_layouts,
                            },
                        );
                    }
                });
            }
            TopLevelDecl::TypeAlias { name, generics, ty } => {
                self.with_pushed_generics(self.current_module, generics.as_deref(), |this| {
                    let type_defs = this.type_defs.clone();
                    if let Some(def) =
                        this.find_def(this.current_module, name.symbol, DefKind::TypeAlias)
                    {
                        let lowered = this.lower_ast_type_with_defs(ty, &type_defs);
                        this.value_types.insert(def, lowered);
                    }
                });
            }
            TopLevelDecl::Function(f) => {
                self.collect_fn_sig(f);
            }
            TopLevelDecl::Impl {
                type_name,
                trait_,
                generics,
                members,
                unsafe_: _impl_unsafe,
                ..
            } => {
                let saved_defs = self.type_defs.clone();
                push_generics(
                    &mut self.type_defs,
                    &self.resolved.defs,
                    self.current_module,
                    generics.as_deref(),
                );
                let impl_type_defs = self.type_defs.clone();
                if let Some(type_def) = self.type_defs.get(&type_name.symbol).copied() {
                    let self_ty = self.impl_self_type_id(type_def, generics.as_deref());
                    let saved_collect_self = self.impl_self_type;
                    self.impl_self_type = Some(self_ty);
                    if let Some(trait_ty) = trait_ {
                        if let Some(inst_key) = self.build_trait_inst_key(
                            type_def,
                            vec![],
                            &trait_ty.inner,
                            &impl_type_defs,
                        ) {
                            if let Some((trait_symbol, _)) = trait_bound_head(&trait_ty.inner) {
                                if let Some(&trait_def) = self.type_defs.get(&trait_symbol) {
                                    self.check_copyable_drop_conflict(
                                        type_def,
                                        trait_def,
                                        trait_ty.span,
                                    );
                                }
                            }
                            self.program_layout.trait_impls.insert(inst_key.clone());
                            let impl_method_names: HashSet<Symbol> = members
                                .iter()
                                .filter_map(|m| match m {
                                    ImplMember::Method(f) => Some(f.name.symbol),
                                    ImplMember::AssociatedType { .. } => None,
                                })
                                .collect();
                            if let Some((trait_symbol, _)) = trait_bound_head(&trait_ty.inner) {
                                if let Some(&trait_def) = self.type_defs.get(&trait_symbol) {
                                    let trait_unsafe =
                                        self.trait_unsafe.get(&trait_def).copied().unwrap_or(false);
                                    if let Some(trait_items) =
                                        self.find_trait_items(trait_def).map(<[TraitItem]>::to_vec)
                                    {
                                        match trait_defaults::synthesize_inherited_methods(
                                            &mut trait_defaults::InheritedSynthesisCtx {
                                                resolved: self.resolved,
                                                trait_items: &trait_items,
                                                impl_method_names: &impl_method_names,
                                                module: self.current_module,
                                                trait_unsafe,
                                                pending_inherited_defs: &mut self
                                                    .pending_inherited_defs,
                                                inherited_trait_methods: &mut self
                                                    .inherited_trait_methods,
                                                inst_key: &inst_key,
                                                inherited_by_inst: &mut self.inherited_by_inst,
                                            },
                                        ) {
                                            Ok(registered) => {
                                                self.register_inherited_trait_method_types(
                                                    trait_def,
                                                    &inst_key,
                                                    &registered,
                                                );
                                                if trait_unsafe {
                                                    for (_, def) in &registered {
                                                        self.mark_fn_effective_unsafe(*def);
                                                    }
                                                }
                                            }
                                            Err(_) => {
                                                self.bag.push(
                                                    self.current_module,
                                                    TypeCheckError::ProgramTooLarge {
                                                        span: trait_ty.span,
                                                    },
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    for member in members {
                        match member {
                            ImplMember::AssociatedType { name, ty } => {
                                if let Some(trait_ty) = trait_ {
                                    if let Some(inst_key) = self.build_trait_inst_key(
                                        type_def,
                                        vec![],
                                        &trait_ty.inner,
                                        &impl_type_defs,
                                    ) {
                                        let concrete =
                                            self.lower_ast_type_with_defs(ty, &impl_type_defs);
                                        self.program_layout
                                            .trait_assoc_impls
                                            .insert((inst_key, name.symbol), concrete);
                                    }
                                }
                            }
                            ImplMember::Method(m) => {
                                self.collect_fn_sig(m);
                                if let Some(fn_def) =
                                    self.find_def(self.current_module, m.name.symbol, DefKind::Fn)
                                {
                                    if let Some(trait_ty) = trait_.as_ref() {
                                        if let Some((trait_symbol, _)) =
                                            trait_bound_head(&trait_ty.inner)
                                        {
                                            if let Some(&trait_def) =
                                                self.type_defs.get(&trait_symbol)
                                            {
                                                if self
                                                    .trait_unsafe
                                                    .get(&trait_def)
                                                    .copied()
                                                    .unwrap_or(false)
                                                {
                                                    self.mark_fn_effective_unsafe(fn_def);
                                                }
                                            }
                                        }
                                        if let Some(inst_key) = self.build_trait_inst_key(
                                            type_def,
                                            vec![],
                                            &trait_ty.inner,
                                            &impl_type_defs,
                                        ) {
                                            self.program_layout
                                                .trait_methods
                                                .insert((inst_key, m.name.symbol), fn_def);
                                        }
                                    } else {
                                        self.program_layout
                                            .inherent_methods
                                            .insert((type_def, m.name.symbol), fn_def);
                                    }
                                }
                            }
                        }
                    }
                    self.impl_self_type = saved_collect_self;
                } else {
                    for member in members {
                        if let ImplMember::Method(m) = member {
                            self.collect_fn_sig(m);
                        }
                    }
                }
                self.type_defs = saved_defs;
            }
            TopLevelDecl::Trait {
                name,
                generics,
                items,
                unsafe_,
                ..
            } => {
                if let Some(trait_def) =
                    self.find_def(self.current_module, name.symbol, DefKind::Trait)
                {
                    self.trait_unsafe.insert(trait_def, *unsafe_);
                }
                self.with_pushed_generics(self.current_module, generics.as_deref(), |this| {
                    let type_defs = this.type_defs.clone();
                    let mut abstract_assoc = HashMap::new();
                    for item in items {
                        if let TraitItem::AssociatedType(assoc_name) = item {
                            if let Some(assoc_def) = this.find_def(
                                this.current_module,
                                assoc_name.symbol,
                                DefKind::TraitAssocType,
                            ) {
                                let abstract_ty = this.types.intern(&Ty::Named {
                                    def: assoc_def,
                                    args: vec![],
                                });
                                abstract_assoc.insert(assoc_name.symbol, abstract_ty);
                                this.type_defs.insert(assoc_name.symbol, assoc_def);
                            }
                        }
                    }
                    let saved_abstract =
                        std::mem::replace(&mut this.trait_assoc_abstract, abstract_assoc);
                    let saved_self = this.impl_self_type;
                    this.impl_self_type = Some(this.types.intern(&Ty::Var(u32::MAX)));
                    for item in items {
                        if let TraitItem::Method(sig) = item {
                            this.collect_fn_sig_only_with_defs(sig, &type_defs);
                        }
                    }
                    this.impl_self_type = saved_self;
                    this.trait_assoc_abstract = saved_abstract;
                });
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
            TopLevelDecl::ExternBlock { items, .. } => {
                for sig in items {
                    self.collect_extern_sig(sig);
                }
            }
            TopLevelDecl::ExternItem { sig, .. } => {
                self.collect_extern_sig(sig);
            }
            _ => {}
        }
    }

    fn collect_fn_sig(&mut self, f: &Function) {
        let fn_ty = self.fn_type_for_function(f);
        if let Some(def) = self.find_def(self.current_module, f.name.symbol, DefKind::Fn) {
            self.value_types.insert(def, fn_ty);
            if f.unsafe_ {
                self.mark_fn_effective_unsafe(def);
            }
        }
    }

    fn collect_fn_sig_only_with_defs(
        &mut self,
        sig: &phx_syntax::ast::decl::FunctionSig,
        type_defs: &TypeDefMap,
    ) {
        let ret = sig
            .ret
            .as_ref()
            .map(|r| self.lower_ast_type_with_defs(r, type_defs))
            .unwrap_or(self.unit);
        let params: Vec<_> = sig
            .params
            .iter()
            .filter_map(|p| match p {
                Param::Named { ty, .. } => Some(self.lower_ast_type_with_defs(ty, type_defs)),
                Param::Receiver { ty, .. } => ty
                    .as_ref()
                    .map(|t| self.lower_ast_type_with_defs(t, type_defs)),
            })
            .collect();
        let fn_ty = self.types.intern(&Ty::Fn { params, ret });
        if let Some(def) = self.find_def(self.current_module, sig.name.symbol, DefKind::Fn) {
            self.value_types.insert(def, fn_ty);
        }
    }

    fn collect_extern_sig(&mut self, sig: &phx_syntax::ast::decl::FunctionSig) {
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
            })
            .collect();
        let fn_ty = self.types.intern(&Ty::Fn { params, ret });
        if let Some(def) = self.find_def(self.current_module, sig.name.symbol, DefKind::ExternFn) {
            self.value_types.insert(def, fn_ty);
        }
    }

    fn trait_subst_for_inst(
        &mut self,
        trait_def: DefId,
        inst_key: &super::layout::TraitInstKey,
    ) -> Substitution {
        let mut trait_subst = Substitution::new();
        let Some(generics) = self.find_trait_generics(trait_def) else {
            return trait_subst;
        };
        let trait_module = self
            .resolved
            .defs
            .get(trait_def.index() as usize)
            .map_or(self.current_module, |d| d.module);
        push_generics(
            &mut self.type_defs,
            &self.resolved.defs,
            trait_module,
            Some(&generics),
        );
        let param_defs = self.generic_param_defs_from_ast(trait_module, &generics);
        for (param, arg) in param_defs.iter().zip(&inst_key.trait_args) {
            trait_subst.insert(*param, *arg);
        }
        trait_subst
    }

    fn register_inherited_trait_method_types(
        &mut self,
        trait_def: DefId,
        inst_key: &super::layout::TraitInstKey,
        registered: &[(Symbol, DefId)],
    ) {
        for (method_sym, fn_def) in registered {
            self.program_layout
                .trait_methods
                .insert((inst_key.clone(), *method_sym), *fn_def);
            let Some(f) = self.inherited_trait_methods.get(fn_def).cloned() else {
                continue;
            };
            let trait_subst = self.trait_subst_for_inst(trait_def, inst_key);
            let mut fn_ty = self.fn_type_for_function(&f);
            if let Ty::Fn { params, ret } = self.types.get(fn_ty).clone() {
                let params: Vec<_> = params
                    .iter()
                    .map(|p| Substitution::apply(&mut self.types, *p, &trait_subst))
                    .collect();
                let ret = Substitution::apply(&mut self.types, ret, &trait_subst);
                fn_ty = self.types.intern(&Ty::Fn { params, ret });
            }
            self.value_types.insert(*fn_def, fn_ty);
        }
    }

    fn check_inherited_trait_methods(
        &mut self,
        inst_key: &super::layout::TraitInstKey,
        inherited: Vec<(DefId, Function)>,
    ) {
        let saved_subst = self.subst.take();
        let saved_type_defs = self.type_defs.clone();
        if let Some((_, trait_def)) = self.active_trait_impl {
            if !inst_key.trait_args.is_empty() {
                let trait_subst = self.trait_subst_for_inst(trait_def, inst_key);
                self.subst = Some(trait_subst);
            }
        }
        for (def, f) in inherited {
            self.check_function_body(&f, def, true, true);
        }
        self.subst = saved_subst;
        self.type_defs = saved_type_defs;
    }

    fn check_top_level(&mut self, item: &TopLevelItem, span: Span) {
        match &item.decl {
            TopLevelDecl::Function(f) => self.check_function(f),
            TopLevelDecl::Const { name, ty, init } => {
                let got = self.check_expr_node(init);
                if let Some(t) = ty {
                    let expected = self.lower_ast_type(t);
                    if !self.types_equal(got, expected) {
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
                self.ownership.define(name.symbol, got);
            }
            TopLevelDecl::Var { name, ty, init } => {
                let expected = self.lower_ast_type(ty);
                let got = self.check_expr_node(init);
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
                self.ownership.define(name.symbol, expected);
            }
            TopLevelDecl::Impl {
                type_name,
                trait_,
                generics,
                members,
                unsafe_: impl_unsafe,
                ..
            } => {
                if *impl_unsafe && trait_.is_none() {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnsafeImplOfSafeTrait {
                            type_name: self.symbol_name(type_name.symbol),
                            span,
                        },
                    );
                } else if let Some(trait_ty) = trait_.as_ref() {
                    self.validate_impl_unsafe(type_name, trait_ty, *impl_unsafe, members, span);
                }
                self.check_impl_decl(type_name, trait_, generics.as_deref(), members, span);
            }
            _ => {}
        }
    }

    fn check_impl_decl(
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
            &self.resolved.defs,
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

    fn check_function(&mut self, f: &Function) {
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
            super::mono::generic_param_defs_for_type(self.resolved, type_def)
                .is_some_and(|params| !params.is_empty())
        }) {
            self.check_function_body(f, def, true, false);
            return;
        }
        self.check_function_body(f, def, true, true);
    }

    fn check_function_body(
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
                    self.define_local(name.symbol, pty, BindingKind::Param, None);
                }
                Param::Receiver { ty, .. } => {
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
                    self.define_local(impl_receiver_symbol(), pty, BindingKind::Param, None);
                }
            }
        }
        if self.impl_self_type.is_some() && !has_receiver {
            let needs_implicit_self = function_body_uses_impl_receiver(&f.body.inner);
            if needs_implicit_self {
                if let Some(self_ty) = self.impl_self_type {
                    self.define_local(impl_receiver_symbol(), self_ty, BindingKind::Param, None);
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

    fn check_block(&mut self, block: &Block) {
        let _ = self.check_block_value(block);
    }

    /// Type-checks `block` and returns the type of its last value-producing item.
    fn check_block_value(&mut self, block: &Block) -> TypeId {
        self.enter_scope();
        let mut last = self.unit;
        for item in &block.items {
            last = match item {
                BlockItem::Stmt(stmt) => self.check_block_stmt_value(&stmt.inner),
                BlockItem::Expr(expr) => self.check_expr_node(expr),
                BlockItem::Import(_) => self.unit,
            };
        }
        self.exit_scope();
        last
    }

    fn check_block_stmt_value(&mut self, stmt: &Stmt) -> TypeId {
        match stmt {
            Stmt::Expr(expr) => {
                if matches!(expr.inner, Expr::Assign { .. }) {
                    let _ = self.check_expr_node(expr);
                    self.unit
                } else {
                    self.check_expr_node(expr)
                }
            }
            Stmt::Return(expr) => self.check_return(expr.as_ref()),
            other => {
                self.check_stmt(other);
                self.unit
            }
        }
    }

    fn check_return(&mut self, expr: Option<&ExprNode>) -> TypeId {
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
                    self.error_mismatch(self.unit, ret, Span::new(0, 0), MismatchKind::Return);
                }
            }
            self.unit
        }
    }

    #[allow(clippy::too_many_lines)]
    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
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
                self.define_local(name.symbol, got, BindingKind::Const, Some(&init.inner));
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
                self.define_local(name.symbol, expected, BindingKind::Var, Some(&init.inner));
            }
            Stmt::Assign { expr } => {
                let _ = self.check_expr_node(expr);
            }
            Stmt::Expr(expr) => {
                let _ = self.check_expr_node(expr);
            }
            Stmt::Return(expr) => {
                let _ = self.check_return(expr.as_ref());
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
    fn check_for_in(&mut self, binding: Ident, iter: &ExprNode, body: &Block, span: Span) {
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
            );
            let option_match_temp = layout.alloc_match_scrutinee_temp(option_ty);
            layout.enter_scope();
            (iter_temp_slot, option_match_temp)
        } else {
            return;
        };
        self.define_local(binding.symbol, item_ty, BindingKind::Var, None);
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

    fn symbol_named(&self, name: &str) -> Option<Symbol> {
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

    fn resolve_trait_def_by_name(&self, name: &str) -> Option<DefId> {
        self.std_trait_kernel
            .trait_def_for_name(&self.resolved.interner, name)
    }

    fn option_ty_for_item(&mut self, item_ty: TypeId) -> Option<TypeId> {
        let option_def = self.std_kernel.option_enum?;
        Some(self.types.intern(&Ty::Named {
            def: option_def,
            args: vec![item_ty],
        }))
    }

    fn some_variant_symbol(&self, _option_ty: TypeId) -> Option<Symbol> {
        let v = self.std_kernel.some_variant?;
        Some(self.resolved.defs.get(v.index() as usize)?.name)
    }

    fn check_assign_expr(&mut self, target: &ExprNode, value: &ExprNode, span: Span) -> TypeId {
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

    fn check_assign_target(&mut self, target: &ExprNode) -> TypeId {
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
                } else if matches!(ops[0], PostfixOp::Index(_)) {
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

    fn move_if_non_copyable(&mut self, init: &ExprNode, ty: TypeId) {
        if self.is_copyable_ty(ty) {
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
        let ty = self.check_expr_with_move(&expr.inner, expr.span, record_move, id);
        self.expr_types.insert(id, ty);
        self.expr_span_types
            .insert((self.current_module, expr.span), ty);
        ty
    }

    fn check_expr_with_move(
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
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::InvalidOperator { op: "unary", span },
                    );
                    self.unit
                })
            }
            Expr::Binary { op, left, right } => {
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
            Literal::String(_) => str_type(&mut self.types),
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

    fn check_path(&mut self, path: &Path, span: Span) -> TypeId {
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

    fn associated_fn_target(&mut self, base: &ExprNode) -> Option<(TypeId, Ident)> {
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

    fn resolve_type_segment_for_assoc_fn(&mut self, segment: &PathSegment) -> Option<TypeId> {
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
    fn check_associated_fn_call(
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
                if let Some(filled) = self.complete_generic_args_from_ast(
                    generic_params_for_def(self.resolved, def).as_deref(),
                    &impl_param_defs,
                    call_generics,
                    self.def_module(def),
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
                .map(|p| Substitution::apply(&mut self.types, *p, &subst))
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
                .map(|p| Substitution::apply(&mut self.types, *p, &subst))
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
            return Substitution::apply(&mut self.types, ret, &subst);
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

    fn check_postfix(&mut self, base: &ExprNode, ops: &[PostfixOp], span: Span) -> TypeId {
        self.check_postfix_with_move(base, ops, span, true, ExprId::from_raw(0))
    }

    fn check_postfix_with_move(
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

    fn check_field(&mut self, base: TypeId, field: &Ident, span: Span) -> TypeId {
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

    fn deref_for_field(&self, ty: TypeId) -> TypeId {
        match self.types.get(ty) {
            Ty::Ref { inner, .. } => *inner,
            _ => ty,
        }
    }

    fn callee_def_from_expr(&self, base: &ExprNode) -> Option<DefId> {
        match &base.inner {
            Expr::Ident(ident) => self.lookup_resolution(ident.id),
            Expr::Path(path) if path.segments.len() == 1 => match &path.segments[0] {
                PathSegment::Ident(ident) => self.lookup_resolution(ident.id),
                PathSegment::Type(seg) => self.lookup_resolution(seg.name.id),
            },
            _ => None,
        }
    }

    fn record_mono_inst(&mut self, base_fn: DefId, args: Vec<TypeId>, site: phx_syntax::AstNodeId) {
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

    fn record_type_mono_inst(&mut self, base_def: DefId, kind: TypeMonoKind, args: Vec<TypeId>) {
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

    fn queue_impl_method_monos(&mut self, type_def: DefId, args: &[TypeId]) {
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
            self.mono_insts.push(MonoInst {
                base_fn,
                args: args.to_vec(),
                call_sites: Vec::new(),
                owner_module: self.current_module,
            });
        }
    }

    /// Ensures monomorphized layout exists when matching on a generic enum scrutinee.
    fn record_scrutinee_type_mono(&mut self, scrutinee: TypeId) {
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

    fn type_mono_kind_for_def(&self, def: DefId) -> Option<TypeMonoKind> {
        let record = self.resolved.defs.get(def.index() as usize)?;
        match record.kind {
            DefKind::Struct => Some(TypeMonoKind::Struct),
            DefKind::Enum => Some(TypeMonoKind::Enum),
            DefKind::TypeAlias => Some(TypeMonoKind::Alias),
            _ => None,
        }
    }

    fn complete_generic_args(
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
                    let concrete = Substitution::apply(&mut this.types, lowered, &subst);
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

    fn complete_generic_args_from_ast(
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
                let concrete = Substitution::apply(&mut this.types, lowered, &subst);
                subst.insert(param_defs[i], concrete);
                provided.push(concrete);
            }
            Some(provided)
        })
    }

    fn resolve_instantiated_named(&mut self, base: DefId, args: Vec<TypeId>, span: Span) -> TypeId {
        let Some(kind) = self.type_mono_kind_for_def(base) else {
            return self.types.intern(&Ty::Named { def: base, args });
        };
        let param_defs = generic_param_defs_for_type(self.resolved, base).unwrap_or_default();
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
            let expanded = Substitution::apply(&mut self.types, body, &subst);
            self.specialized_aliases
                .insert(TypeMonoKey::new(base, args), expanded);
            return expanded;
        }
        self.types.intern(&Ty::Named { def: base, args })
    }

    fn struct_fields_for_named(&mut self, def: DefId, args: &[TypeId]) -> HashMap<Symbol, TypeId> {
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
        template
            .fields
            .iter()
            .map(|(name, ty)| (*name, Substitution::apply(&mut self.types, *ty, &subst)))
            .collect()
    }

    fn check_tuple_struct_cast(&mut self, from: TypeId, to: TypeId) -> bool {
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

    fn single_field_tuple_inner(&mut self, ty: TypeId) -> Option<TypeId> {
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

    fn tuple_field_symbol(&self, index: usize) -> Option<Symbol> {
        let name = index.to_string();
        self.resolved.interner.lookup(&name)
    }

    fn enum_def_for_variant(&self, variant_def: DefId) -> Option<DefId> {
        self.program_layout
            .variants
            .get(&variant_def)
            .map(|meta| meta.enum_def)
    }

    fn check_tuple_struct_ctor_call(
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

    fn substituted_variant_payload(
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
                    .map(|t| Substitution::apply(&mut self.types, *t, &subst))
                    .collect();
                VariantKind::Tuple(pts)
            }
            VariantKind::Struct(fs) => {
                let fields: Vec<_> = fs
                    .iter()
                    .map(|(n, t)| (*n, Substitution::apply(&mut self.types, *t, &subst)))
                    .collect();
                VariantKind::Struct(fields)
            }
        })
    }

    fn generic_param_defs_for_fn(
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

    fn generic_param_defs_from_ast(
        &self,
        module: u32,
        params: &[phx_syntax::ast::types::GenericParam],
    ) -> Vec<DefId> {
        params
            .iter()
            .filter_map(|param| self.find_def(module, param.name.symbol, DefKind::GenericParam))
            .collect()
    }

    fn generic_param_bounds_for_def(&self, param_def: DefId) -> Option<Vec<Node<Type>>> {
        let record = self.resolved.defs.get(param_def.index() as usize)?;
        let module = record.module;
        let name = record.name;
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

    fn find_trait_method_for_bounded_generic_param(
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
                    if sig.name.symbol == method {
                        let trait_module = self.resolved.defs[trait_def.index() as usize].module;
                        return self.find_def(trait_module, method, DefKind::Fn);
                    }
                }
            }
        }
        None
    }

    fn find_trait_items(&self, trait_def: DefId) -> Option<&[TraitItem]> {
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

    fn find_trait_generics(
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

    fn build_trait_inst_key(
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

    fn trait_def_for_type(&self, trait_ty: &Type) -> Option<DefId> {
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

    fn check_trait_impl_exhaustiveness(
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
        let type_display = self.symbol_name(type_name.symbol);
        let trait_display = self.symbol_name(trait_symbol);
        for item in trait_items {
            match item {
                TraitItem::AssociatedType(name) if !impl_assoc.contains(&name.symbol) => {
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
                    if sig.body.is_some() || impl_methods.contains(&sig.name.symbol) {
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

    fn find_inherent_impl_generics(
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

    fn impl_generic_param_defs(&self, type_def: DefId) -> Vec<DefId> {
        let module = self
            .resolved
            .defs
            .get(type_def.index() as usize)
            .map_or(self.current_module, |d| d.module);
        self.find_inherent_impl_generics(type_def)
            .map(|params| self.generic_param_defs_from_ast(module, &params))
            .unwrap_or_default()
    }

    fn receiver_type_args(&self, type_def: DefId, receiver: TypeId) -> Option<Vec<TypeId>> {
        let Ty::Named { def, args } = self.types.get(receiver).clone() else {
            return None;
        };
        if def != type_def {
            return None;
        }
        Some(args)
    }

    fn method_receiver_matches(&self, param: TypeId, receiver: TypeId) -> bool {
        if self.types_equal(receiver, param) {
            return true;
        }
        if let Ty::Ref { inner, .. } = self.types.get(param) {
            return self.types_equal(receiver, *inner);
        }
        false
    }

    fn method_arg_param_types(
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

    fn find_function_decl(&self, def: DefId) -> Option<&Function> {
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

    fn context_generic_args_for_enum(&self, enum_def: DefId) -> Option<Vec<TypeId>> {
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

    fn check_try_expr(&mut self, scrutinee_ty: TypeId, span: Span, expr_id: ExprId) -> TypeId {
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

    fn check_result_try_expr(
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

    fn record_try_site(
        &mut self,
        expr_id: ExprId,
        scrutinee_ty: TypeId,
        success_ty: TypeId,
        failure_mode: TryFailureMode,
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
        let temp_slot = layout.alloc_match_scrutinee_temp(scrutinee_ty);
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
    fn resolve_concrete_generic_args(
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
            return self.with_pushed_generics(fn_module, fn_generics, |this| {
                let type_defs = this.type_defs.clone();
                if nodes.len() > param_defs.len() {
                    this.bag.push(
                        this.current_module,
                        TypeCheckError::ArityMismatch {
                            expected: param_defs.len(),
                            found: nodes.len(),
                            span,
                        },
                    );
                    return None;
                }
                if nodes.len() < param_defs.len() {
                    return this.complete_generic_args_from_ast(
                        fn_generics,
                        param_defs,
                        nodes,
                        fn_module,
                        span,
                    );
                }
                let mut concrete_args = Vec::new();
                for ty_node in nodes {
                    concrete_args.push(this.lower_ast_type_with_defs(ty_node, &type_defs));
                }
                Some(concrete_args)
            });
        }
        let mut infer = InferenceCtx::new();
        let mut subst = Substitution::new();
        for param_def in param_defs {
            let var = infer.fresh_var(&mut self.types);
            subst.insert(*param_def, var);
        }
        let applied: Vec<_> = param_types
            .iter()
            .map(|p| Substitution::apply(&mut self.types, *p, &subst))
            .collect();
        let mut arg_types = Vec::with_capacity(args.len());
        for arg in args {
            arg_types.push(self.check_expr_node_read(arg));
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

    fn check_call_with_generics(
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
            .map(|p| Substitution::apply(&mut self.types, *p, &subst))
            .collect();
        let ret = Substitution::apply(&mut self.types, ret, &subst);
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

    fn check_enum_variant_call_with_generics(
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

    fn check_call(&mut self, callee: TypeId, args: &[ExprNode], span: Span) -> TypeId {
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

    fn check_index(&mut self, base: TypeId, span: Span) -> TypeId {
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

    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    fn check_method_call_with_generics(
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
        let Ty::Named {
            def,
            args: type_args,
        } = self.types.get(receiver).clone()
        else {
            return self.emit_unresolved_method(receiver, name, span);
        };
        let type_def = def;
        let implementer_args = type_args;
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
        self.method_call_sites.insert(site_id, fn_def);
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
                .map(|p| Substitution::apply(&mut self.types, *p, &subst))
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
                .map(|p| Substitution::apply(&mut self.types, *p, &subst))
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
            self.record_mono_inst(fn_def, mono_args, name.id);
            let out = Substitution::apply(&mut self.types, ret, &subst);
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
        ret
    }

    fn check_primitive_method_call(
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

    fn emit_unresolved_method(&mut self, receiver: TypeId, name: &Ident, span: Span) -> TypeId {
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

    fn emit_ambiguous_or_unresolved_method(
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

    fn check_if(
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

    fn check_if_arm(&mut self, condition: &IfCondition, then_block: &BlockNode) -> TypeId {
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
                    let _ = layout.alloc_match_scrutinee_temp(s);
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

    fn check_match(
        &mut self,
        scrutinee: &ExprNode,
        arms: &[phx_syntax::ast::pat::MatchArm],
        span: Span,
    ) -> TypeId {
        let s = self.check_expr_node(scrutinee);
        self.record_scrutinee_type_mono(s);
        if let Some(layout) = &mut self.layout {
            let _ = layout.alloc_match_scrutinee_temp(s);
        }
        let pre = self.ownership.clone();
        let mut arm_states = Vec::new();
        let mut acc: Option<TypeId> = None;
        for arm in arms {
            let (body_ty, end) = self.check_with_ownership_fork(&pre, |this| {
                this.enter_scope();
                this.check_pattern(&arm.pattern.inner, s, arm.pattern.span, BindingKind::Var);
                if let Some(g) = &arm.guard {
                    let gt = this.check_expr_node(g);
                    if !this.types_equal(gt, this.bool_ty) {
                        this.error_mismatch(this.bool_ty, gt, g.span, MismatchKind::Condition);
                    }
                }
                let body_ty = this.check_expr_node(&arm.body);
                this.exit_scope();
                body_ty
            });
            arm_states.push(end);
            acc = Some(match acc {
                None => body_ty,
                Some(prev) => unify_branch(&self.alias_env(), prev, body_ty).unwrap_or_else(|| {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::NonUnifyingBranches { span },
                    );
                    self.unit
                }),
            });
        }
        self.ownership = OwnershipTracker::join_arms(&pre, &arm_states);
        self.check_match_unreachable_arms(s, arms);
        self.check_match_exhaustiveness(s, arms, span);
        acc.unwrap_or(self.unit)
    }

    fn check_match_unreachable_arms(
        &mut self,
        scrutinee: TypeId,
        arms: &[phx_syntax::ast::pat::MatchArm],
    ) {
        let is_enum = self.scrutinee_enum_def(scrutinee).is_some();
        let mut after_unconditional_wildcard = false;
        let mut covered_variants = std::collections::HashSet::new();
        let mut covered_literals: Vec<phx_syntax::ast::lit::Literal> = Vec::new();

        for arm in arms {
            let pat_span = arm.pattern.span;
            if after_unconditional_wildcard {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::UnreachableMatchArm {
                        reason: "a previous `_` arm matches all remaining values",
                        span: pat_span,
                    },
                );
                continue;
            }

            match &arm.pattern.inner {
                Pattern::Wildcard if arm.guard.is_none() => {
                    after_unconditional_wildcard = true;
                }
                Pattern::Literal(lit) => {
                    if covered_literals
                        .iter()
                        .any(|prev| pattern_literal_eq(prev, lit))
                    {
                        self.bag.push(
                            self.current_module,
                            TypeCheckError::UnreachableMatchArm {
                                reason: "an earlier arm already matches this literal",
                                span: pat_span,
                            },
                        );
                    } else {
                        covered_literals.push(lit.clone());
                    }
                }
                _ if is_enum => {
                    if let Some(variant) = self.pattern_covered_variant(&arm.pattern.inner) {
                        if !covered_variants.insert(variant) {
                            self.bag.push(
                                self.current_module,
                                TypeCheckError::UnreachableMatchArm {
                                    reason: "an earlier arm already matches this enum variant",
                                    span: pat_span,
                                },
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn symbol_name(&self, symbol: Symbol) -> String {
        self.resolved.interner.resolve_display(symbol)
    }

    fn check_match_exhaustiveness(
        &mut self,
        scrutinee: TypeId,
        arms: &[phx_syntax::ast::pat::MatchArm],
        span: Span,
    ) {
        if arms
            .iter()
            .any(|arm| matches!(arm.pattern.inner, Pattern::Wildcard))
        {
            return;
        }

        if let Some(enum_def) = self.scrutinee_enum_def(scrutinee) {
            let Some(layout) = self.program_layout.enums.get(&enum_def) else {
                return;
            };
            let mut covered = std::collections::HashSet::new();
            for arm in arms {
                if let Some(variant_name) = self.pattern_covered_variant(&arm.pattern.inner) {
                    covered.insert(variant_name);
                }
            }
            let missing: Vec<String> = layout
                .variants
                .iter()
                .filter(|v| !covered.contains(&v.name))
                .map(|v| self.symbol_name(v.name))
                .collect();
            if !missing.is_empty() {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::NonExhaustiveMatch { missing, span },
                );
            }
            return;
        }

        match self.types.get(scrutinee) {
            Ty::Primitive(Keyword::Bool) => {
                let mut has_true = false;
                let mut has_false = false;
                for arm in arms {
                    if let Pattern::Literal(Literal::Bool(value)) = &arm.pattern.inner {
                        if *value {
                            has_true = true;
                        } else {
                            has_false = true;
                        }
                    }
                }
                let mut missing = Vec::new();
                if !has_true {
                    missing.push("true".to_string());
                }
                if !has_false {
                    missing.push("false".to_string());
                }
                if !missing.is_empty() {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::NonExhaustiveMatch { missing, span },
                    );
                }
            }
            Ty::Primitive(kw) if is_int_keyword(*kw) => {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::NonExhaustiveMatch {
                        missing: vec!["_".to_string()],
                        span,
                    },
                );
            }
            _ => {}
        }
    }

    fn pattern_covered_variant(&self, pat: &Pattern) -> Option<Symbol> {
        match pat {
            Pattern::Wildcard | Pattern::Literal(_) => None,
            Pattern::Ident(ident) => self
                .program_layout
                .enum_variant_by_name(ident.symbol)
                .map(|(_, v)| v.name),
            Pattern::Struct { name, .. } | Pattern::Tuple { name, .. } => self
                .program_layout
                .enum_variant_by_name(name.symbol)
                .map(|(_, v)| v.name),
            Pattern::Range { .. } => None,
        }
    }

    fn scrutinee_enum_def(&self, scrutinee: TypeId) -> Option<DefId> {
        match self.types.get(scrutinee) {
            Ty::Named { def, .. } if self.program_layout.enums.contains_key(def) => Some(*def),
            _ => None,
        }
    }

    fn named_type_args(&self, ty: TypeId) -> Option<(DefId, Vec<TypeId>)> {
        match self.types.get(ty) {
            Ty::Named { def, args } => Some((*def, args.clone())),
            _ => None,
        }
    }

    fn variant_payload_for_scrutinee(
        &mut self,
        variant_def: DefId,
        scrutinee: TypeId,
    ) -> Option<VariantKind> {
        let (_, args) = self.named_type_args(scrutinee)?;
        if args.is_empty() {
            return self
                .program_layout
                .variants
                .get(&variant_def)
                .map(|meta| meta.payload.clone());
        }
        self.substituted_variant_payload(variant_def, &args)
    }

    fn error_enum_pattern_on_non_enum(&mut self, scrutinee: TypeId, span: Span) {
        self.bag.push(
            self.current_module,
            TypeCheckError::Mismatch {
                expected: "enum".to_string(),
                found: self.format_ty(scrutinee),
                span,
                kind: MismatchKind::default(),
            },
        );
    }

    fn error_enum_variant_mismatch(&mut self, expected_def: DefId, scrutinee: TypeId, span: Span) {
        self.bag.push(
            self.current_module,
            TypeCheckError::Mismatch {
                expected: self.format_named(expected_def),
                found: self.format_ty(scrutinee),
                span,
                kind: MismatchKind::default(),
            },
        );
    }

    fn check_pattern(
        &mut self,
        pat: &Pattern,
        scrutinee: TypeId,
        span: Span,
        binding_kind: BindingKind,
    ) {
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
                    self.define_local(ident.symbol, scrutinee, binding_kind, None);
                }
            }
            Pattern::Struct { name, fields } => {
                if let Some(&def) = self.type_defs.get(&name.symbol) {
                    if let Ty::Named { def: sdef, .. } = self.types.get(scrutinee) {
                        if *sdef != def {
                            self.bag.push(
                                self.current_module,
                                TypeCheckError::Mismatch {
                                    expected: self.format_named(def),
                                    found: self.format_ty(scrutinee),
                                    span,
                                    kind: MismatchKind::default(),
                                },
                            );
                        }
                    }
                    for field in fields {
                        if let Some(fty) = self
                            .struct_fields
                            .get(&def)
                            .and_then(|sf| sf.fields.get(&field.name.symbol).copied())
                        {
                            if let Some(p) = &field.pattern {
                                self.check_pattern(&p.inner, fty, span, binding_kind);
                            } else {
                                self.define_local(field.name.symbol, fty, binding_kind, None);
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
                    if let Some(VariantKind::Struct(payload)) =
                        self.variant_payload_for_scrutinee(variant.def, scrutinee)
                    {
                        let field_map: HashMap<Symbol, TypeId> = payload.iter().copied().collect();
                        for field in fields {
                            if let Some(fty) = field_map.get(&field.name.symbol) {
                                if let Some(p) = &field.pattern {
                                    self.check_pattern(&p.inner, *fty, span, binding_kind);
                                } else {
                                    self.define_local(field.name.symbol, *fty, binding_kind, None);
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
                    if let Some(VariantKind::Tuple(payload)) =
                        self.variant_payload_for_scrutinee(variant.def, scrutinee)
                    {
                        for (p, pty) in patterns.iter().zip(payload.iter()) {
                            self.check_pattern(&p.inner, *pty, span, binding_kind);
                        }
                    }
                }
            }
            Pattern::Range { start, end, .. } => {
                let _ = self.check_expr_node(start);
                let _ = self.check_expr_node(end);
                self.push_unsupported("range pattern", span);
            }
        }
    }

    fn format_named(&self, def: DefId) -> String {
        self.resolved
            .defs
            .get(def.index() as usize)
            .map(|d| format!("type#{}", d.name.index()))
            .unwrap_or_else(|| "<?>".to_string())
    }

    #[allow(clippy::too_many_lines)]
    fn check_struct_lit(
        &mut self,
        name: &TypeName,
        generics: Option<&[phx_syntax::ast::Node<phx_syntax::ast::Type>]>,
        fields: &[StructFieldInit],
        span: Span,
    ) -> TypeId {
        if let Some(&def) = self.type_defs.get(&name.symbol) {
            if self.resolved.defs[def.index() as usize].kind == DefKind::Struct {
                return self.check_struct_type_lit(def, generics, fields, span);
            }
        }
        if let Some((enum_def, variant)) = self.program_layout.enum_variant_by_name(name.symbol) {
            return self.check_enum_struct_variant_lit(enum_def, variant, generics, fields, span);
        }
        if let Some(&def) = self.type_defs.get(&name.symbol) {
            return self.check_struct_type_lit(def, generics, fields, span);
        }
        self.bag.push(
            self.current_module,
            TypeCheckError::UnknownType {
                symbol_index: name.symbol.index(),
                span,
            },
        );
        self.unit
    }

    fn struct_lit_type_args(
        &mut self,
        def: DefId,
        generics: Option<&[phx_syntax::ast::Node<phx_syntax::ast::Type>]>,
        span: Span,
        missing_feature: &'static str,
    ) -> Vec<TypeId> {
        let param_defs = generic_param_defs_for_type(self.resolved, def).unwrap_or_default();
        if param_defs.is_empty() {
            if generics.is_some() {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::UnsupportedFeature {
                        feature: "type arguments on non-generic struct literal",
                        span,
                    },
                );
            }
            return vec![];
        }
        let Some(generic_nodes) = generics else {
            self.bag.push(
                self.current_module,
                TypeCheckError::UnsupportedFeature {
                    feature: missing_feature,
                    span,
                },
            );
            return vec![];
        };
        if generic_nodes.len() > param_defs.len() {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: param_defs.len(),
                    found: generic_nodes.len(),
                    span,
                },
            );
            return vec![];
        }
        if generic_nodes.len() < param_defs.len() {
            return self
                .complete_generic_args_from_ast(
                    generic_params_for_def(self.resolved, def).as_deref(),
                    &param_defs,
                    generic_nodes,
                    self.def_module(def),
                    span,
                )
                .unwrap_or_default();
        }
        generic_nodes
            .iter()
            .map(|ty_node| self.lower_ast_type(ty_node))
            .collect()
    }

    fn check_struct_type_lit(
        &mut self,
        def: DefId,
        generics: Option<&[phx_syntax::ast::Node<phx_syntax::ast::Type>]>,
        fields: &[StructFieldInit],
        span: Span,
    ) -> TypeId {
        let type_args = self.struct_lit_type_args(
            def,
            generics,
            span,
            "missing explicit type arguments on generic struct literal",
        );
        let ty = if type_args.is_empty() {
            self.types.intern(&Ty::Named { def, args: vec![] })
        } else {
            self.resolve_instantiated_named(def, type_args.clone(), span)
        };
        if self.program_layout.structs.contains_key(&def) {
            let field_map = self.struct_fields_for_named(def, &type_args);
            let required_fields: Vec<Symbol> = field_map.keys().copied().collect();
            for field in fields {
                if matches!(field, StructFieldInit::Spread(_)) {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnsupportedFeature {
                            feature: "struct literal spread",
                            span,
                        },
                    );
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
                    if !self.types_equal(got, *expected) {
                        self.error_mismatch(
                            *expected,
                            got,
                            value.span,
                            MismatchKind::StructField {
                                name: self.symbol_name(fname.symbol),
                            },
                        );
                    }
                } else {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnknownStructField {
                            name: self.symbol_name(fname.symbol),
                            span: value.span,
                        },
                    );
                }
            }
            for fname in required_fields {
                if !seen.contains(&fname) {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::MissingStructField {
                            name: self.symbol_name(fname),
                            span,
                        },
                    );
                }
            }
        }
        ty
    }

    fn check_enum_struct_variant_lit(
        &mut self,
        enum_def: DefId,
        variant: super::layout::VariantLayout,
        generics: Option<&[phx_syntax::ast::Node<phx_syntax::ast::Type>]>,
        fields: &[StructFieldInit],
        span: Span,
    ) -> TypeId {
        let type_args = self.struct_lit_type_args(
            enum_def,
            generics,
            span,
            "missing explicit type arguments on generic enum struct literal",
        );
        if !type_args.is_empty() {
            self.record_type_mono_inst(enum_def, TypeMonoKind::Enum, type_args.clone());
        }
        let enum_ty = if type_args.is_empty() {
            self.types.intern(&Ty::Named {
                def: enum_def,
                args: vec![],
            })
        } else {
            self.types.intern(&Ty::Named {
                def: enum_def,
                args: type_args.clone(),
            })
        };
        let payload = self
            .substituted_variant_payload(variant.def, &type_args)
            .unwrap_or(variant.kind);
        if let VariantKind::Struct(payload) = payload {
            let field_map: HashMap<Symbol, TypeId> = payload.iter().copied().collect();
            let required_fields: Vec<Symbol> = payload.iter().map(|(n, _)| *n).collect();
            for field in fields {
                if matches!(field, StructFieldInit::Spread(_)) {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnsupportedFeature {
                            feature: "enum struct literal spread",
                            span,
                        },
                    );
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
                    if !self.types_equal(got, *expected) {
                        self.error_mismatch(
                            *expected,
                            got,
                            value.span,
                            MismatchKind::EnumVariantField {
                                name: self.symbol_name(fname.symbol),
                            },
                        );
                    }
                } else {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnknownEnumVariantField {
                            name: self.symbol_name(fname.symbol),
                            span: value.span,
                        },
                    );
                }
            }
            for fname in required_fields {
                if !seen.contains(&fname) {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::MissingEnumVariantField {
                            name: self.symbol_name(fname),
                            span,
                        },
                    );
                }
            }
        }
        enum_ty
    }

    #[allow(clippy::type_complexity)]
    fn finish(
        self,
    ) -> (
        TypeInterner,
        HashMap<ExprId, TypeId>,
        HashMap<(u32, Span), TypeId>,
        TypeCheckBag,
        Vec<FunctionLayout>,
        ProgramLayout,
        HashMap<TypeMonoKey, TypeId>,
        StdKernel,
        StdTraitKernel,
        HashMap<ExprId, TrySiteMeta>,
        HashMap<ExprId, PrimitiveMethodSite>,
        HashMap<ExprId, DefId>,
        HashMap<ExprId, DefId>,
        HashMap<DefId, TypeId>,
        HashMap<ExprId, IndirectCallMeta>,
        HashMap<ExprId, IntrinsicSite>,
        HashMap<ExprId, u32>,
        IntrinsicKernel,
        Vec<crate::resolver::Def>,
        trait_defaults::InheritedTraitMethods,
        HashMap<DefId, bool>,
    ) {
        (
            self.types,
            self.expr_types,
            self.expr_span_types,
            self.bag,
            self.functions,
            self.program_layout,
            self.specialized_aliases,
            self.std_kernel,
            self.std_trait_kernel,
            self.try_sites,
            self.primitive_method_sites,
            self.associated_fn_sites,
            self.method_call_sites,
            self.value_types,
            self.indirect_call_sites,
            self.intrinsic_call_sites,
            self.size_of_literals,
            self.intrinsic_kernel,
            self.pending_inherited_defs,
            self.inherited_trait_methods,
            self.fn_effective_unsafe,
        )
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn finish_all(
        self,
    ) -> (
        TypeInterner,
        HashMap<ExprId, TypeId>,
        TypeCheckBag,
        Vec<FunctionLayout>,
        ProgramLayout,
        HashMap<DefId, TypeId>,
        HashMap<TypeMonoKey, TypeId>,
        HashMap<ExprId, TrySiteMeta>,
        HashMap<ExprId, DefId>,
        HashMap<ExprId, DefId>,
        HashMap<ExprId, IndirectCallMeta>,
        HashMap<ExprId, IntrinsicSite>,
        HashMap<ExprId, u32>,
    ) {
        (
            self.types,
            self.expr_types,
            self.bag,
            self.functions,
            self.program_layout,
            self.value_types,
            self.specialized_aliases,
            self.try_sites,
            self.associated_fn_sites,
            self.method_call_sites,
            self.indirect_call_sites,
            self.intrinsic_call_sites,
            self.size_of_literals,
        )
    }

    fn is_borrow_type(&self, ty: TypeId) -> bool {
        matches!(self.types.get(ty), Ty::Slice(_) | Ty::Str | Ty::Ref { .. })
    }

    fn check_utf8_array_to_str_cast(&self, from: TypeId, to: TypeId, expr: &Expr) -> bool {
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

    fn binding_kind_for_ident(&self, ident: Ident) -> Option<BindingKind> {
        self.layout.as_ref()?.binding(ident.symbol).map(|b| b.kind)
    }

    fn local_binding_escapes(kind: BindingKind) -> bool {
        matches!(
            kind,
            BindingKind::Var | BindingKind::Const | BindingKind::MatchTemp
        )
    }

    fn expr_borrow_site(&mut self, expr: &Expr) -> Option<Span> {
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

    fn check_expr_escapes_local(&mut self, expr: &ExprNode) {
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

fn drop_prim_kind_byte(types: &TypeInterner, ty: TypeId) -> u8 {
    if matches!(types.get(ty), Ty::Fn { .. }) {
        SLOT_KIND_FN_PTR
    } else {
        primitive_kind_for_type(types, ty).map_or(SLOT_KIND_AGG, phx_bytecode::PrimitiveKind::as_u8)
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

impl TypeChecker<'_> {
    /// Registers `const` / `var` slots for layout emission without type-checking bodies.
    ///
    /// Generic impl method templates defer body checking to monomorphization; lowering still
    /// needs local slots for annotated bindings in the template AST.
    fn collect_layout_bindings_block(&mut self, block: &Block, type_defs: &TypeDefMap) {
        self.enter_scope();
        for item in &block.items {
            match item {
                BlockItem::Stmt(stmt) => match &stmt.inner {
                    Stmt::Const { name, ty, .. } => {
                        let pty = ty
                            .as_ref()
                            .map(|t| self.lower_ast_type_with_defs(t, type_defs))
                            .unwrap_or(self.unit);
                        self.define_local(name.symbol, pty, BindingKind::Const, None);
                    }
                    Stmt::Var { name, ty, .. } => {
                        let pty = self.lower_ast_type_with_defs(ty, type_defs);
                        self.define_local(name.symbol, pty, BindingKind::Var, None);
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

    fn collect_layout_bindings_from_expr(&mut self, expr: &ExprNode, type_defs: &TypeDefMap) {
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
fn function_body_uses_impl_receiver(block: &Block) -> bool {
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

fn find_trait_method_def(
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

fn generic_bounds_in_decl(decl: &TopLevelDecl, name: Symbol) -> Option<Vec<Node<Type>>> {
    match decl {
        TopLevelDecl::Struct { generics, .. }
        | TopLevelDecl::Enum { generics, .. }
        | TopLevelDecl::TypeAlias { generics, .. }
        | TopLevelDecl::Trait { generics, .. }
        | TopLevelDecl::Impl { generics, .. } => {
            generic_bounds_in_params(generics.as_deref(), name)
        }
        TopLevelDecl::Function(f) => generic_bounds_in_params(f.generics.as_deref(), name),
        _ => None,
    }
}

fn generic_bounds_in_params(
    generics: Option<&[GenericParam]>,
    name: Symbol,
) -> Option<Vec<Node<Type>>> {
    let params = generics?;
    for param in params {
        if param.name.symbol == name {
            return param.bounds.clone();
        }
    }
    None
}

fn trailing_value_expr(block: &Block) -> Option<&ExprNode> {
    for item in block.items.iter().rev() {
        match item {
            BlockItem::Expr(expr) => return Some(expr),
            BlockItem::Stmt(stmt) => match &stmt.inner {
                Stmt::Return(expr) => return expr.as_ref(),
                Stmt::Expr(expr) if !matches!(expr.inner, Expr::Assign { .. }) => {
                    return Some(expr);
                }
                _ => {}
            },
            BlockItem::Import(_) => {}
        }
    }
    None
}

fn pattern_literal_eq(a: &Literal, b: &Literal) -> bool {
    match (a, b) {
        (Literal::Int(x), Literal::Int(y)) => x.value == y.value && x.suffix == y.suffix,
        (Literal::Float(x), Literal::Float(y)) => x.value == y.value && x.suffix == y.suffix,
        (Literal::Bool(x), Literal::Bool(y)) => x == y,
        (Literal::ByteChar(x), Literal::ByteChar(y)) => x == y,
        (Literal::ByteString(x), Literal::ByteString(y)) => x == y,
        (Literal::String(x), Literal::String(y)) => x == y,
        _ => false,
    }
}

/// Runs type checking on `resolved`.
///
/// # Errors
///
/// Returns [`TypeCheckBag`] when one or more type errors were collected.
pub fn type_check(mut resolved: ResolvedProgram) -> Result<super::TypedProgram, TypeCheckBag> {
    let mut checker = TypeChecker::new(&resolved);
    checker.check_program();
    let mono_insts = checker.take_mono_insts();
    let type_mono_insts = checker.take_type_mono_insts();
    let (
        types,
        expr_types,
        expr_span_types,
        bag,
        functions,
        layout,
        specialized_aliases,
        std_kernel,
        std_trait_kernel,
        try_sites,
        primitive_method_sites,
        associated_fn_sites,
        method_call_sites,
        value_types,
        indirect_call_sites,
        intrinsic_call_sites,
        size_of_literals,
        intrinsic_kernel,
        pending_inherited_defs,
        inherited_trait_methods,
        fn_effective_unsafe,
    ) = checker.finish();
    if bag.has_errors() {
        return Err(bag);
    }
    trait_defaults::merge_pending_inherited_defs(&mut resolved, pending_inherited_defs);
    let entry = resolved.main_fn;
    let mut program = super::TypedProgram {
        resolved,
        types,
        expr_types,
        expr_span_types,
        functions,
        entry,
        layout,
        specialized_from: HashMap::new(),
        mono_insts: Vec::new(),
        specialized_aliases,
        std_kernel,
        std_trait_kernel,
        try_sites,
        primitive_method_sites,
        associated_fn_sites,
        method_call_sites,
        value_types,
        indirect_call_sites,
        intrinsic_call_sites,
        size_of_literals,
        intrinsic_kernel,
        inherited_trait_methods,
        fn_effective_unsafe,
    };
    let mono_bag = super::mono::monomorphize(&mut program, &mono_insts, &type_mono_insts);
    if mono_bag.has_errors() {
        return Err(mono_bag);
    }
    program.mono_insts = mono_insts;
    Ok(program)
}

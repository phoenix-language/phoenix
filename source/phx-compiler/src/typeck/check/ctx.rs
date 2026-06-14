//! Shared type-checker context: constructors, scope, errors, and finish.

use std::collections::HashMap;

use phx_diagnostics::{MismatchKind, Span, TypeCheckBag, TypeCheckError};
use phx_syntax::Symbol;
use phx_syntax::ast::decl::Function;
use phx_syntax::ast::expr::Expr;
use phx_syntax::ast::lit::Literal;

use super::StructFields;
use super::TypeChecker;
use crate::resolver::{DefId, DefKind, ResolvedProgram};
use crate::typeck::IndirectCallMeta;
use crate::typeck::PrimitiveMethodSite;
use crate::typeck::bindings::{BindingKind, FunctionLayout};
use crate::typeck::builtins::{bool_type, implements_drop, is_copyable, unit};
use crate::typeck::display::{format_type, format_type_diagnostic};
use crate::typeck::intrinsic_kernel::{IntrinsicKernel, IntrinsicSite};
use crate::typeck::layout::{ProgramLayout, TypeMonoKey};
use crate::typeck::lower_ty::{build_type_def_map, error_type, push_generics};
use crate::typeck::mono::{MonoInst, TypeMonoInst};
use crate::typeck::ownership::OwnershipTracker;
use crate::typeck::std_kernel::{StdKernel, TrySiteMeta};
use crate::typeck::std_trait_kernel::StdTraitKernel;
use crate::typeck::subst::Substitution;
use crate::typeck::trait_defaults;
use crate::typeck::types::{ExprId, Ty, TypeId, TypeInterner};
use crate::typeck::unify::AliasEnv;

impl<'a> TypeChecker<'a> {
    pub(in crate::typeck::check) fn new(resolved: &'a ResolvedProgram) -> Self {
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

    pub(in crate::typeck::check) fn new_with_types(
        resolved: &'a ResolvedProgram,
        mut types: TypeInterner,
    ) -> Self {
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
    pub(in crate::typeck::check) fn is_effective_unsafe(&self, def: DefId) -> bool {
        self.fn_effective_unsafe.get(&def).copied().unwrap_or(false)
    }

    pub(in crate::typeck::check) fn mark_fn_effective_unsafe(&mut self, def: DefId) {
        self.fn_effective_unsafe.insert(def, true);
    }
    pub(in crate::typeck::check) fn is_copyable_ty(&self, ty: TypeId) -> bool {
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
    pub(in crate::typeck::check) fn with_pushed_generics<R>(
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
    pub(in crate::typeck::check) fn alloc_type_id(&mut self, def: DefId) -> u32 {
        let id = self.next_type_id;
        self.next_type_id += 1;
        self.program_layout.type_ids.insert(def, id);
        id
    }
    pub(in crate::typeck::check) fn with_unsafe<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.unsafe_depth = self.unsafe_depth.saturating_add(1);
        f(self);
        self.unsafe_depth = self.unsafe_depth.saturating_sub(1);
    }
    pub(in crate::typeck::check) fn alloc_fn_sig_type_id(&mut self, fn_ty: TypeId) -> u32 {
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

    pub(in crate::typeck::check) fn check_with_ownership_fork<R>(
        &mut self,
        pre: &OwnershipTracker,
        f: impl FnOnce(&mut Self) -> R,
    ) -> (R, OwnershipTracker) {
        self.ownership = pre.clone();
        let result = f(self);
        let end = self.ownership.clone();
        (result, end)
    }

    pub(in crate::typeck::check) fn enter_scope(&mut self) {
        self.ownership.enter_scope();
        if let Some(layout) = &mut self.layout {
            layout.enter_scope();
        }
    }

    pub(in crate::typeck::check) fn exit_scope(&mut self) {
        let exiting = self.layout_scope_depth();
        self.plan_drops_at_scope_depth(exiting);
        self.ownership.exit_scope();
        if let Some(layout) = &mut self.layout {
            layout.exit_scope();
        }
    }

    pub(in crate::typeck::check) fn fn_def_for(&self, f: &Function) -> Option<DefId> {
        self.find_def(self.current_module, f.name.symbol, DefKind::Fn)
    }

    pub(in crate::typeck::check) fn alias_env(&self) -> AliasEnv<'_> {
        AliasEnv {
            types: &self.types,
            defs: &self.resolved.defs,
            value_types: &self.value_types,
        }
    }

    pub(in crate::typeck::check) fn types_equal(&self, a: TypeId, b: TypeId) -> bool {
        crate::typeck::unify::same_type(&self.alias_env(), a, b)
    }

    pub(in crate::typeck::check) fn utf8_rodata_for_const_init(init: &Expr) -> Option<Vec<u8>> {
        if let Expr::Literal(Literal::ByteString(b)) = init {
            if std::str::from_utf8(b).is_ok() {
                return Some(b.clone());
            }
        }
        None
    }

    pub(in crate::typeck::check) fn define_local(
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

    pub(in crate::typeck::check) fn alloc_expr_id(&mut self) -> ExprId {
        let id = ExprId::from_raw(self.next_expr);
        self.next_expr += 1;
        id
    }

    pub(in crate::typeck::check) fn format_ty(&self, id: TypeId) -> String {
        format_type(
            &self.types,
            &self.resolved.interner,
            &self.resolved.defs,
            id,
        )
    }

    pub(in crate::typeck::check) fn format_ty_diagnostic(&self, id: TypeId) -> String {
        format_type_diagnostic(
            &self.types,
            &self.resolved.interner,
            &self.resolved.defs,
            id,
        )
    }

    pub(in crate::typeck::check) fn error_mismatch(
        &mut self,
        expected: TypeId,
        found: TypeId,
        span: Span,
        kind: MismatchKind,
    ) {
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

    pub(in crate::typeck::check) fn poison_type(&mut self) -> TypeId {
        error_type(&mut self.types)
    }

    pub(in crate::typeck::check) fn symbol_name(&self, symbol: Symbol) -> String {
        self.resolved.interner.resolve_display(symbol)
    }

    #[allow(clippy::type_complexity)]
    pub(in crate::typeck::check) fn finish(
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
}

//! Type-checking driver and AST walk.

// AST enums are `#[non_exhaustive]`; wildcard arms reserve future variants.
#![allow(unreachable_patterns)]

use std::collections::HashMap;

use phx_diagnostics::{MismatchKind, Span, TypeCheckBag, TypeCheckError};
use phx_syntax::ast::decl::{
    Function, Param, StructBody, TopLevelDecl, TopLevelItem, TraitItem, Variant,
};
use phx_syntax::ast::expr::{Expr, PostfixOp, StructFieldInit, UnaryOp};
use phx_syntax::ast::ident::{Ident, Path, PathSegment, TypeName};
use phx_syntax::ast::lit::Literal;
use phx_syntax::ast::pat::{MatchArm, Pattern};
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};
use phx_syntax::ast::types::Type;
use phx_syntax::ast::{BlockNode, ExprNode, Node};
use phx_syntax::{Symbol, impl_receiver_symbol};

use super::bindings::{BindingKind, FunctionLayout, FunctionLayoutBuilder};
use super::builtins::{
    bool_type, float_literal_type, int_literal_type, is_copyable, str_type, u8_type, unit,
};
use super::display::{format_type, format_type_diagnostic};
use super::infer::InferenceCtx;
use super::layout::{
    EnumLayout, ProgramLayout, StructLayout, TypeMonoKey, VariantKind, VariantLayout, VariantMeta,
};
use super::lower_ty::{TypeDefMap, build_type_def_map, error_type, lower_type, push_generics};
use super::mono::{
    MonoInst, TypeMonoInst, TypeMonoKind, generic_param_defs_for_type, generic_params_for_def,
};
use super::ops::{check_binary, check_cast, check_unary};
use super::ownership::OwnershipTracker;
use super::primitive::is_int_keyword;
use super::subst::Substitution;
use super::types::is_error_type;
use super::types::{ExprId, Ty, TypeId, TypeInterner};
use super::unify::AliasEnv;
use super::unify::unify_branch;
use crate::resolver::{DefId, DefKind, ResolutionKey, ResolvedProgram};
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
    /// Active type substitution when checking a monomorphized clone.
    subst: Option<Substitution>,
    /// Explicit generic instantiations to specialize after the main pass.
    mono_insts: Vec<MonoInst>,
    /// Explicit generic type instantiations to specialize after the main pass.
    type_mono_insts: Vec<TypeMonoInst>,
    /// Expanded alias types keyed by monomorphization key (filled during checking).
    specialized_aliases: HashMap<TypeMonoKey, TypeId>,
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
            program_layout: ProgramLayout::default(),
            next_type_id: 1,
            impl_self_type: None,
            current_module: resolved.root,
            subst: None,
            mono_insts: Vec::new(),
            type_mono_insts: Vec::new(),
            specialized_aliases: HashMap::new(),
        }
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

    pub(crate) fn check_function_specialized(
        &mut self,
        f: &Function,
        spec_def: DefId,
        base_fn: DefId,
        mono_args: &[TypeId],
    ) {
        let fn_ty = self.fn_type_for_function(f);
        self.value_types.insert(spec_def, fn_ty);
        if let Ty::Fn { ret, .. } = self.types.get(fn_ty).clone() {
            self.fn_ret = Some(ret);
        }
        let method_is_generic = f.generics.as_ref().is_some_and(|g| !g.is_empty());
        let on_generic_impl = self.impl_type_for_method(base_fn).is_some_and(|type_def| {
            self.find_inherent_impl_generics(type_def)
                .is_some_and(|params| !params.is_empty())
        });
        if on_generic_impl && !method_is_generic {
            self.fn_ret = None;
            return;
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
        self.check_function_body(f, spec_def, true);
        self.impl_self_type = saved_impl_self;
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
                    type_name,
                    members,
                    trait_,
                    ..
                } = &item.inner.decl
                {
                    if trait_.is_some() {
                        continue;
                    }
                    if members.iter().any(|m| self.fn_def_for(m) == Some(fn_def)) {
                        return self.find_def(module.id, type_name.symbol, DefKind::Struct);
                    }
                }
            }
        }
        None
    }

    fn fn_type_for_function(&mut self, f: &Function) -> TypeId {
        let mut td = self.type_defs.clone();
        push_generics(&mut td, &self.resolved.defs, f.generics.as_deref());
        let ret = f
            .ret
            .as_ref()
            .map(|r| self.lower_ast_type_with_defs(r, &td))
            .unwrap_or(self.unit);
        let params: Vec<_> = f
            .params
            .iter()
            .filter_map(|p| match p {
                Param::Named { ty, .. } => Some(self.lower_ast_type_with_defs(ty, &td)),
                Param::Receiver { ty, .. } => {
                    ty.as_ref().map(|t| self.lower_ast_type_with_defs(t, &td))
                }
                _ => None,
            })
            .collect();
        self.types.intern(&Ty::Fn { params, ret })
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

    fn with_loop_body<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.loop_depth = self.loop_depth.saturating_add(1);
        f(self);
        self.loop_depth = self.loop_depth.saturating_sub(1);
    }

    fn enter_scope(&mut self) {
        self.ownership.enter_scope();
        if let Some(layout) = &mut self.layout {
            layout.enter_scope();
        }
    }

    fn exit_scope(&mut self) {
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

    fn is_post_mvp_std_type_name(&self, symbol: Symbol) -> bool {
        let name = self.resolved.interner.resolve(symbol);
        if name != "Option" && name != "Result" {
            return false;
        }
        !self.type_defs.contains_key(&symbol)
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
        self.resolved.interner.resolve(symbol) == "Self"
    }

    fn lower_ast_type_with_defs(&mut self, ty: &Node<Type>, type_defs: &TypeDefMap) -> TypeId {
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
            if self.is_post_mvp_std_type_name(name.symbol) {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::UnsupportedFeature {
                        feature: "std Option/Result types (post-MVP)",
                        span: ty.span,
                    },
                );
                return self.poison_type();
            }
        }
        let id = lower_type(&mut self.types, type_defs, &ty.inner);
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
                    if !m.derives.is_empty() {
                        self.push_unsupported("#derive on impl method", m.body.span);
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
            self.push_unsupported("#derive directive", span);
        }
    }

    fn push_unsupported(&mut self, feature: &'static str, span: Span) {
        self.bag.push(
            self.current_module,
            TypeCheckError::UnsupportedFeature { feature, span },
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
                derives,
                generics,
                variants,
            } => {
                let _ = derives;
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
                generics,
                members,
                ..
            } => {
                let saved_defs = self.type_defs.clone();
                push_generics(
                    &mut self.type_defs,
                    &self.resolved.defs,
                    generics.as_deref(),
                );
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
                self.type_defs = saved_defs;
            }
            TopLevelDecl::Trait {
                generics, items, ..
            } => {
                let mut td = self.type_defs.clone();
                push_generics(&mut td, &self.resolved.defs, generics.as_deref());
                for item in items {
                    if let TraitItem::Method(sig) = item {
                        self.collect_fn_sig_only_with_defs(sig, &td);
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
        let fn_ty = self.fn_type_for_function(f);
        if let Some(def) = self.find_def(self.current_module, f.name.symbol, DefKind::Fn) {
            self.value_types.insert(def, fn_ty);
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
                _ => None,
            })
            .collect();
        let fn_ty = self.types.intern(&Ty::Fn { params, ret });
        if let Some(def) = self.find_def(self.current_module, sig.name.symbol, DefKind::Fn) {
            self.value_types.insert(def, fn_ty);
        }
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
                ..
            } => {
                if let Some(trait_name) = trait_ {
                    self.check_trait_impl_exhaustiveness(type_name, trait_name, members, span);
                }
                let saved_defs = self.type_defs.clone();
                push_generics(
                    &mut self.type_defs,
                    &self.resolved.defs,
                    generics.as_deref(),
                );
                if let Some(&type_def) = self.type_defs.get(&type_name.symbol) {
                    let self_ty = self.impl_self_type_id(type_def, generics.as_deref());
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
                self.type_defs = saved_defs;
            }
            _ => {}
        }
    }

    fn check_function(&mut self, f: &Function) {
        if !f.derives.is_empty() {
            self.push_unsupported("#derive directive", f.body.span);
        }
        let is_generic = f.generics.as_ref().is_some_and(|g| !g.is_empty());
        if is_generic {
            return;
        }
        let def = self.fn_def_for(f).unwrap_or(DefId::from_raw(0));
        self.check_function_body(f, def, true);
    }

    fn check_function_body(&mut self, f: &Function, def: DefId, emit_layout: bool) {
        let mut td = self.type_defs.clone();
        push_generics(&mut td, &self.resolved.defs, f.generics.as_deref());
        let ret = self.fn_ret.unwrap_or_else(|| {
            f.ret
                .as_ref()
                .map(|r| self.lower_ast_type_with_defs(r, &td))
                .unwrap_or(self.unit)
        });
        self.fn_ret = Some(ret);
        self.ownership = OwnershipTracker::new();
        if emit_layout {
            self.layout = Some(FunctionLayoutBuilder::new(def, ret));
        }
        let expr_start = self.next_expr;
        let has_receiver = f.params.iter().any(|p| matches!(p, Param::Receiver { .. }));
        for p in &f.params {
            match p {
                Param::Named { name, ty, .. } => {
                    let pty = self.lower_ast_type_with_defs(ty, &td);
                    self.define_local(name.symbol, pty, BindingKind::Param, None);
                }
                Param::Receiver { ty, .. } => {
                    let pty = ty
                        .as_ref()
                        .map(|t| self.lower_ast_type_with_defs(t, &td))
                        .or(self.impl_self_type)
                        .unwrap_or(self.unit);
                    self.define_local(impl_receiver_symbol(), pty, BindingKind::Param, None);
                }
                _ => {}
            }
        }
        if self.impl_self_type.is_some() && !has_receiver {
            if let Some(self_ty) = self.impl_self_type {
                self.define_local(impl_receiver_symbol(), self_ty, BindingKind::Param, None);
            }
        }
        let body_ty = self.check_block_value(&f.body.inner);
        if !self.types_equal(body_ty, ret) {
            self.error_mismatch(ret, body_ty, f.body.span, MismatchKind::FunctionBody);
        }
        if self.is_borrow_type(body_ty) {
            if let Some(expr) = trailing_value_expr(&f.body.inner) {
                self.check_expr_escapes_local(expr);
            }
        }
        if emit_layout {
            if let Some(mut builder) = self.layout.take() {
                builder.set_expr_range(expr_start, self.next_expr);
                self.functions.push(builder.finish());
            }
        }
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
                BlockItem::Stmt(stmt) => self.check_block_stmt_value(stmt),
                BlockItem::Expr(expr) => self.check_expr_node(expr),
                BlockItem::Import(_) => self.unit,
                _ => self.unit,
            };
        }
        self.exit_scope();
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
                self.define_local(name.symbol, expected, BindingKind::Var, Some(&init.inner));
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
            Stmt::Break { value, span } => {
                if self.loop_depth == 0 {
                    self.error_loop_control_outside_loop("break", *span);
                } else if value.is_some() {
                    self.push_unsupported("break with value", *span);
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
                self.with_loop_body(|this| this.check_block(&body.inner));
            }
            Stmt::ForIn { iter, body, .. } => {
                self.push_unsupported("for-in loop", iter.span);
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
                self.check_given_exhaustiveness(s, &pattern.inner, pattern.span);
                self.check_block(&body.inner);
            }
            Stmt::Unsafe(body) => self.check_block(&body.inner),
            _ => {}
        }
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
                let td = self.type_defs.clone();
                let to = self.lower_ast_type_with_defs(ty, &td);
                if !check_cast(&self.alias_env(), from, to)
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
                cond,
                then_block,
                else_ifs,
                else_block,
            } => self.check_if(cond, then_block, else_ifs, else_block, span),
            Expr::Match { scrutinee, arms } => self.check_match(scrutinee, arms, span),
            Expr::Block(block) => self.check_block_expr(block),
            Expr::StructLit {
                name,
                generics,
                fields,
            } => self.check_struct_lit(name, generics.as_deref(), fields, span),
            Expr::Unsafe(block) => self.check_block_expr(block),
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
                    _ => span,
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
                    _ => "runtime directive",
                };
                for arg in args {
                    let _ = self.check_expr_node(arg);
                }
                self.push_unsupported(feature, span);
                self.unit
            }
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
            Literal::String(_) => str_type(&mut self.types),
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
            if record_move && !is_copyable(&self.types, ty) {
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
                PathSegment::Type(name) => {
                    if let Some(def) = self.lookup_resolution(name.id) {
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
                PostfixOp::Method {
                    name,
                    generics,
                    args,
                    ..
                } => {
                    self.check_method_call_with_generics(ty, name, generics.as_deref(), args, span)
                }
                PostfixOp::Call { generics, args } => {
                    let callee_def = self.callee_def_from_expr(base);
                    self.check_call_with_generics(
                        ty,
                        callee_def,
                        base,
                        generics.as_deref(),
                        args,
                        span,
                    )
                }
                PostfixOp::Index(idx) => {
                    let _ = self.check_expr_node(idx);
                    self.check_index(ty, span)
                }
                PostfixOp::Try => {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnsupportedFeature {
                            feature: "`?` operator (requires std `Option` / `Result`)",
                            span,
                        },
                    );
                    self.unit
                }
                _ => self.unit,
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
                PathSegment::Type(name) => self.lookup_resolution(name.id),
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
        self.type_mono_insts.push(TypeMonoInst {
            base_def,
            kind,
            args,
        });
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
        if param_defs.len() != args.len() {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: param_defs.len(),
                    found: args.len(),
                    span,
                },
            );
            return self.poison_type();
        }
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

    fn enum_def_for_variant(&self, variant_def: DefId) -> Option<DefId> {
        self.program_layout
            .variants
            .get(&variant_def)
            .map(|meta| meta.enum_def)
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

    fn generic_param_defs_for_fn(&self, f: &phx_syntax::ast::decl::Function) -> Vec<DefId> {
        let Some(params) = f.generics.as_ref() else {
            return Vec::new();
        };
        self.generic_param_defs_from_ast(params)
    }

    fn generic_param_defs_from_ast(
        &self,
        params: &[phx_syntax::ast::types::GenericParam],
    ) -> Vec<DefId> {
        params
            .iter()
            .filter_map(|param| {
                self.find_def(
                    self.current_module,
                    param.name.symbol,
                    DefKind::GenericParam,
                )
            })
            .collect()
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

    fn check_trait_impl_exhaustiveness(
        &mut self,
        type_name: &TypeName,
        trait_name: &TypeName,
        members: &[Function],
        span: Span,
    ) {
        let Some(trait_def) = self.type_defs.get(&trait_name.symbol).copied() else {
            return;
        };
        let Some(trait_items) = self.find_trait_items(trait_def) else {
            return;
        };
        let impl_methods: std::collections::HashSet<Symbol> =
            members.iter().map(|m| m.name.symbol).collect();
        let type_display = self.resolved.interner.resolve(type_name.symbol).to_owned();
        let trait_display = self.resolved.interner.resolve(trait_name.symbol).to_owned();
        let missing_methods: Vec<Symbol> = trait_items
            .iter()
            .filter_map(|item| {
                let TraitItem::Method(sig) = item else {
                    return None;
                };
                if sig.body.is_some() || impl_methods.contains(&sig.name.symbol) {
                    return None;
                }
                Some(sig.name.symbol)
            })
            .collect();
        for method_symbol in missing_methods {
            let method_display = self.resolved.interner.resolve(method_symbol).to_owned();
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
                    if let Some(struct_def) =
                        self.find_def(module.id, type_name.symbol, DefKind::Struct)
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
        self.find_inherent_impl_generics(type_def)
            .map(|params| self.generic_param_defs_from_ast(&params))
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
        for module in &self.resolved.modules {
            for item in &module.program.items {
                match &item.inner.decl {
                    TopLevelDecl::Function(f) if self.fn_def_for(f) == Some(def) => return Some(f),
                    TopLevelDecl::Impl { members, .. } => {
                        for m in members {
                            if self.fn_def_for(m) == Some(def) {
                                return Some(m);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    fn resolve_concrete_generic_args(
        &mut self,
        generic_nodes: Option<&[Node<Type>]>,
        fn_generics: Option<&[phx_syntax::ast::types::GenericParam]>,
        param_defs: &[DefId],
        param_types: &[TypeId],
        args: &[ExprNode],
        span: Span,
    ) -> Option<Vec<TypeId>> {
        let mut td = self.type_defs.clone();
        push_generics(&mut td, &self.resolved.defs, fn_generics);
        if let Some(nodes) = generic_nodes {
            if nodes.len() != param_defs.len() {
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
            let mut concrete_args = Vec::new();
            for (param_def, ty_node) in param_defs.iter().zip(nodes) {
                let concrete = self.lower_ast_type_with_defs(ty_node, &td);
                concrete_args.push(concrete);
                let _ = param_def;
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
        let Some(f) = self.find_function_decl(fn_def) else {
            if let Some(enum_def) = self.enum_def_for_variant(fn_def) {
                return self.check_enum_variant_call_with_generics(
                    enum_def, fn_def, callee, generics, args, span,
                );
            }
            return self.check_call(callee, args, span);
        };
        let param_defs = self.generic_param_defs_for_fn(f);
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
        let Some(concrete_args) = self.resolve_concrete_generic_args(
            generics,
            generic_params_for_def(self.resolved, enum_def).as_deref(),
            &param_defs,
            &ctor_params,
            args,
            span,
        ) else {
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

    #[allow(clippy::too_many_lines)]
    fn check_method_call_with_generics(
        &mut self,
        receiver: TypeId,
        name: &Ident,
        generics: Option<&[Node<Type>]>,
        args: &[ExprNode],
        span: Span,
    ) -> TypeId {
        let Ty::Named { def, .. } = self.types.get(receiver) else {
            return self.emit_unresolved_method(receiver, name, span);
        };
        let type_def = *def;
        let fn_def = self
            .program_layout
            .inherent_methods
            .get(&(type_def, name.symbol))
            .copied()
            .or_else(|| find_trait_method_def(&self.program_layout, type_def, name.symbol));
        let Some(fn_def) = fn_def else {
            return self.emit_ambiguous_or_unresolved_method(receiver, type_def, name, span);
        };
        let Some(&fn_ty) = self.value_types.get(&fn_def) else {
            return self.emit_unresolved_method(receiver, name, span);
        };
        let Some(f) = self.find_function_decl(fn_def).cloned() else {
            return self.check_call(fn_ty, args, span);
        };
        let impl_param_defs = self.impl_generic_param_defs(type_def);
        let method_param_defs = self.generic_param_defs_for_fn(&f);
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
            return Substitution::apply(&mut self.types, ret, &subst);
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
        ret
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
            .filter(|((t, _trait_def, method), _)| *t == type_def && *method == name.symbol)
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
        cond: &ExprNode,
        then_block: &BlockNode,
        else_ifs: &[(ExprNode, BlockNode)],
        else_block: &Option<BlockNode>,
        span: Span,
    ) -> TypeId {
        let c = self.check_expr_node(cond);
        if !self.types_equal(c, self.bool_ty) {
            self.error_mismatch(self.bool_ty, c, cond.span, MismatchKind::Condition);
        }
        let mut then_ty = self.check_block_expr(then_block);
        for (ec, eb) in else_ifs {
            let e = self.check_expr_node(ec);
            if !self.types_equal(e, self.bool_ty) {
                self.error_mismatch(self.bool_ty, e, ec.span, MismatchKind::Condition);
            }
            let arm_ty = self.check_block_expr(eb);
            then_ty = unify_branch(&self.alias_env(), then_ty, arm_ty).unwrap_or_else(|| {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::NonUnifyingBranches { span },
                );
                self.unit
            });
        }
        if let Some(else_b) = else_block {
            let arm_ty = self.check_block_expr(else_b);
            then_ty = unify_branch(&self.alias_env(), then_ty, arm_ty).unwrap_or_else(|| {
                self.bag.push(
                    self.current_module,
                    TypeCheckError::NonUnifyingBranches { span },
                );
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
                if !self.types_equal(gt, self.bool_ty) {
                    self.error_mismatch(self.bool_ty, gt, g.span, MismatchKind::Condition);
                }
            }
            let body_ty = self.check_expr_node(&arm.body);
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
        self.resolved.interner.resolve(symbol).to_owned()
    }

    fn check_given_exhaustiveness(&mut self, scrutinee: TypeId, pat: &Pattern, span: Span) {
        let arm = MatchArm {
            pattern: Node::new(pat.clone(), span, phx_syntax::AstNodeId::synthetic(0)),
            guard: None,
            body: Node::new(
                Expr::Literal(Literal::Int(phx_syntax::ast::lit::IntLit {
                    value: 0,
                    suffix: phx_syntax::token::IntegerSuffix::None,
                })),
                span,
                phx_syntax::AstNodeId::synthetic(1),
            ),
        };
        self.check_match_exhaustiveness(scrutinee, std::slice::from_ref(&arm), span);
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
            _ => None,
        }
    }

    fn scrutinee_enum_def(&self, scrutinee: TypeId) -> Option<DefId> {
        match self.types.get(scrutinee) {
            Ty::Named { def, .. } if self.program_layout.enums.contains_key(def) => Some(*def),
            _ => None,
        }
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
                    self.define_local(ident.symbol, scrutinee, BindingKind::Var, None);
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
                                self.check_pattern(&p.inner, fty, span);
                            } else {
                                self.define_local(field.name.symbol, fty, BindingKind::Var, None);
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
                                    self.define_local(
                                        field.name.symbol,
                                        *fty,
                                        BindingKind::Var,
                                        None,
                                    );
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
        if generic_nodes.len() != param_defs.len() {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: param_defs.len(),
                    found: generic_nodes.len(),
                    span,
                },
            );
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
        TypeCheckBag,
        Vec<FunctionLayout>,
        ProgramLayout,
        HashMap<TypeMonoKey, TypeId>,
    ) {
        (
            self.types,
            self.expr_types,
            self.bag,
            self.functions,
            self.program_layout,
            self.specialized_aliases,
        )
    }

    #[allow(clippy::type_complexity)]
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
    ) {
        (
            self.types,
            self.expr_types,
            self.bag,
            self.functions,
            self.program_layout,
            self.value_types,
            self.specialized_aliases,
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
                let td = self.type_defs.clone();
                let to = self.lower_ast_type_with_defs(ty, &td);
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

fn callee_name_use_id(base: &ExprNode) -> Option<phx_syntax::AstNodeId> {
    match &base.inner {
        Expr::Ident(ident) => Some(ident.id),
        Expr::Path(path) if path.segments.len() == 1 => match &path.segments[0] {
            PathSegment::Ident(ident) => Some(ident.id),
            PathSegment::Type(name) => Some(name.id),
        },
        _ => None,
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

fn trailing_value_expr(block: &Block) -> Option<&ExprNode> {
    for item in block.items.iter().rev() {
        match item {
            BlockItem::Expr(expr) => return Some(expr),
            BlockItem::Stmt(Stmt::Return(expr)) => return expr.as_ref(),
            BlockItem::Stmt(Stmt::Expr(expr)) if !matches!(expr.inner, Expr::Assign { .. }) => {
                return Some(expr);
            }
            _ => {}
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
pub fn type_check(resolved: &ResolvedProgram) -> Result<super::TypedProgram, TypeCheckBag> {
    let mut checker = TypeChecker::new(resolved);
    checker.check_program();
    let mono_insts = checker.take_mono_insts();
    let type_mono_insts = checker.take_type_mono_insts();
    let (types, expr_types, bag, functions, layout, specialized_aliases) = checker.finish();
    if bag.has_errors() {
        return Err(bag);
    }
    let mut program = super::TypedProgram {
        resolved: resolved.clone(),
        types,
        expr_types,
        functions,
        entry: resolved.main_fn,
        layout,
        specialized_from: HashMap::new(),
        specialized_aliases,
    };
    let mono_bag = super::mono::monomorphize(&mut program, &mono_insts, &type_mono_insts);
    if mono_bag.has_errors() {
        return Err(mono_bag);
    }
    Ok(program)
}

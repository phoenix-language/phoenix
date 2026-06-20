//! Top-level declaration collection and checking.
//!
//! First pass over the resolved program: lower AST type syntax into [`TypeId`]s, register struct
//! and enum layouts, collect function signatures, and seed [`crate::typeck::layout::ProgramLayout`]
//! before [`super::stmt`] and [`super::impls`] check bodies.
//!
//! # Two-phase orchestration
//!
//! 1. **Collection** — [`TypeChecker::collect_decls`] walks every module and calls
//!    [`TypeChecker::collect_top_level_decl`] to populate `type_defs`, `struct_fields`,
//!    `value_types`, trait/impl metadata, and layout tables without entering function bodies.
//! 2. **Checking** — [`TypeChecker::check_program`] builds the lang-item registry, validates
//!    type-alias cycles, then [`TypeChecker::check_top_level`] dispatches each item (functions,
//!    consts, impl blocks) to body checkers in sibling modules.
//!
//! # Key entry points
//!
//! | Function | Role |
//! |----------|------|
//! | [`TypeChecker::check_program`] | Run collection, alias validation, and top-level checking. |
//! | [`TypeChecker::collect_decls`] | Module-wide declaration collection pass. |
//! | [`TypeChecker::collect_top_level_decl`] | Register one struct/enum/trait/impl/fn signature. |
//! | [`TypeChecker::check_top_level`] | Check one top-level item after collection. |
//! | [`TypeChecker::lower_ast_type_with_defs`] | Lower AST [`Type`] using a frozen generic map. |
//! | [`TypeChecker::validate_type_aliases`] | Detect cyclic type-alias definitions. |
//! | [`trailing_value_expr`] | Find the block expression that supplies a trailing value. |

use std::collections::{HashMap, HashSet};

use phx_diagnostics::{MismatchKind, Span, TypeCheckError};
use phx_syntax::Symbol;
use phx_syntax::ast::decl::{
    Function, ImplMember, Param, StructBody, TopLevelDecl, TopLevelItem, TraitItem, Variant,
};
use phx_syntax::ast::expr::Expr;
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};
use phx_syntax::ast::types::GenericParam;
use phx_syntax::ast::types::Type;
use phx_syntax::ast::{ExprNode, Node};

use super::StructFields;
use super::TypeChecker;
use crate::lang_items::build_lang_item_registry;
use crate::resolver::{DefId, DefKind, ResolutionKey};
use crate::typeck::bounds::trait_bound_head;
use crate::typeck::layout::{
    EnumLayout, StructLayout, TraitImplementer, VariantKind, VariantLayout, VariantMeta,
};
use crate::typeck::lower_ty::{TypeDefMap, lower_type, push_generics};
use crate::typeck::subst::Substitution;
use crate::typeck::trait_defaults;
use crate::typeck::types::is_error_type;
use crate::typeck::types::{Ty, TypeId};

impl TypeChecker<'_> {
    fn lower_resolved_named_type(
        &mut self,
        name: &phx_syntax::ast::ident::TypeName,
        generics: &Option<Vec<Node<Type>>>,
        type_defs: &TypeDefMap,
        span: Span,
    ) -> Option<TypeId> {
        let def = self.lookup_resolution(name.id)?;
        let args = generics
            .as_ref()
            .map(|gs| {
                gs.iter()
                    .map(|g| self.lower_ast_type_with_defs(g, type_defs))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if args.is_empty() {
            let id = self.types.intern(&Ty::Named {
                def,
                args: Vec::new(),
            });
            return Some(if let Some(subst) = &self.subst {
                Substitution::apply(&mut self.types, id, subst, self.resolved)
            } else {
                id
            });
        }
        Some(self.resolve_instantiated_named(def, args, span))
    }

    fn lower_self_assoc_type(
        &mut self,
        member: &phx_syntax::ast::ident::TypeName,
        span: Span,
    ) -> TypeId {
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
                span,
            },
        );
        self.poison_type()
    }

    fn lower_named_type_prefix(
        &mut self,
        name: &phx_syntax::ast::ident::TypeName,
        generics: &Option<Vec<Node<Type>>>,
        type_defs: &TypeDefMap,
        span: Span,
    ) -> Option<TypeId> {
        if self.is_self_type_name(name.symbol) {
            if let Some(self_ty) = self.impl_self_type {
                if let Some(gs) = generics {
                    if !gs.is_empty() {
                        self.bag.push(
                            self.current_module,
                            TypeCheckError::UnsupportedFeature {
                                feature: "generic arguments on Self",
                                span,
                            },
                        );
                    }
                }
                return Some(self_ty);
            }
        }
        if let Some(id) = self.lower_resolved_named_type(name, generics, type_defs, span) {
            return Some(id);
        }
        let args = generics
            .as_ref()
            .map(|gs| {
                gs.iter()
                    .map(|g| self.lower_ast_type_with_defs(g, type_defs))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let def = self.type_defs.get(&name.symbol).copied()?;
        if args.is_empty() {
            let id = self.types.intern(&Ty::Named {
                def,
                args: Vec::new(),
            });
            return Some(if let Some(subst) = &self.subst {
                Substitution::apply(&mut self.types, id, subst, self.resolved)
            } else {
                id
            });
        }
        Some(self.resolve_instantiated_named(def, args, span))
    }

    /// Lowers an AST [`Type`] node to a [`TypeId`] using `type_defs` for generic parameter lookup.
    ///
    /// Handles `Self`, associated types in impl/trait context, references, tuples, and named types
    /// (with optional generic arguments). Errors are recorded in the diagnostic bag and a poison
    /// type is returned when resolution fails.
    pub(in crate::typeck::check) fn lower_ast_type_with_defs(
        &mut self,
        ty: &Node<Type>,
        type_defs: &TypeDefMap,
    ) -> TypeId {
        if let Type::SelfAssoc { member } = &ty.inner {
            return self.lower_self_assoc_type(member, ty.span);
        }
        if let Type::Named { name, generics } = &ty.inner {
            if let Some(id) = self.lower_named_type_prefix(name, generics, type_defs, ty.span) {
                return id;
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
            Substitution::apply(&mut self.types, id, subst, self.resolved)
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

    /// Reports cyclic type-alias chains reachable from each alias definition.
    pub(in crate::typeck::check) fn validate_type_aliases(&mut self) {
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

    pub(in crate::typeck::check) fn type_alias_cycle_from(
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

    /// Lowers an AST [`Type`] using the checker's current [`TypeDefMap`](crate::typeck::lower_ty::TypeDefMap).
    pub(in crate::typeck::check) fn lower_ast_type(&mut self, ty: &Node<Type>) -> TypeId {
        let type_defs = self.type_defs.clone();
        self.lower_ast_type_with_defs(ty, &type_defs)
    }

    pub(in crate::typeck::check) fn find_def(
        &self,
        module: u32,
        name: Symbol,
        kind: DefKind,
    ) -> Option<DefId> {
        self.resolved
            .defs
            .iter()
            .enumerate()
            .find(|(_, d)| d.module == module && d.name == name && d.kind == kind)
            .map(|(i, _)| DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)))
    }

    pub(in crate::typeck::check) fn find_function_def(
        &self,
        module: u32,
        name: Symbol,
    ) -> Option<DefId> {
        self.resolved.defs.iter().enumerate().find_map(|(i, d)| {
            if d.module == module && d.name == name && d.kind.is_function_body() {
                Some(DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)))
            } else {
                None
            }
        })
    }

    pub(in crate::typeck::check) fn lookup_resolution(
        &self,
        node_id: phx_syntax::AstNodeId,
    ) -> Option<DefId> {
        self.resolved
            .resolutions
            .get(&ResolutionKey {
                module: self.current_module,
                node_id,
            })
            .copied()
    }

    /// Runs declaration collection, lang-item setup, alias validation, and top-level checking.
    ///
    /// Called once from [`super::type_check`](crate::typeck::check::type_check) after name
    /// resolution. Does not monomorphize; that runs after the checker finishes successfully.
    pub(in crate::typeck::check) fn check_program(&mut self) {
        self.collect_decls();
        self.lang_items = build_lang_item_registry(self.resolved, &mut self.bag);
        self.validate_type_aliases();
        for module in &self.resolved.modules {
            self.current_module = module.id;
            for item in &module.program.items {
                self.check_decl_derives(&item.inner, item.span);
                self.check_top_level(&item.inner, item.span);
            }
        }
    }

    pub(in crate::typeck::check) fn check_decl_derives(&mut self, item: &TopLevelItem, span: Span) {
        let derives = match &item.decl {
            TopLevelDecl::Struct { derives, .. }
            | TopLevelDecl::Enum { derives, .. }
            | TopLevelDecl::Trait { derives, .. } => derives,
            TopLevelDecl::Function(f) => &f.derives,
            TopLevelDecl::Impl { members, .. } => {
                for m in members {
                    if let ImplMember::Method(f) = m {
                        if !f.derives.is_empty() {
                            self.push_unsupported("#[derive] on impl method", f.body.span);
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

    pub(in crate::typeck::check) fn push_unsupported(&mut self, feature: &'static str, span: Span) {
        self.bag.push(
            self.current_module,
            TypeCheckError::UnsupportedFeature { feature, span },
        );
    }

    pub(in crate::typeck::check) fn push_internal_error(
        &mut self,
        detail: &'static str,
        span: Span,
    ) {
        self.bag.push(
            self.current_module,
            TypeCheckError::InternalError { detail, span },
        );
    }

    /// Walks all modules and collects top-level declarations without checking bodies.
    pub(in crate::typeck::check) fn collect_decls(&mut self) {
        for module in &self.resolved.modules {
            self.current_module = module.id;
            for item in &module.program.items {
                self.collect_top_level_decl(&item.inner.decl);
            }
        }
    }

    /// Registers layouts, signatures, and trait metadata for one [`TopLevelDecl`].
    ///
    /// Struct and enum variants populate [`StructFields`](super::StructFields) and
    /// [`ProgramLayout`](crate::typeck::layout::ProgramLayout); functions store fn types in
    /// `value_types` via [`Self::collect_fn_sig`].
    #[allow(clippy::too_many_lines)]
    pub(in crate::typeck::check) fn collect_top_level_decl(&mut self, decl: &TopLevelDecl) {
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
                    self.resolved,
                    self.current_module,
                    generics.as_deref(),
                );
                let impl_type_defs = self.type_defs.clone();
                if let Some(implementer) = self.trait_implementer_for_type_name(type_name) {
                    let self_ty =
                        self.self_ty_for_trait_implementer(implementer, generics.as_deref());
                    let saved_collect_self = self.impl_self_type;
                    self.impl_self_type = Some(self_ty);
                    if let Some(trait_ty) = trait_ {
                        if let Some(inst_key) = self.build_trait_inst_key(
                            implementer,
                            vec![],
                            &trait_ty.inner,
                            &impl_type_defs,
                        ) {
                            if let TraitImplementer::Type(type_def) = implementer {
                                if let Some((trait_symbol, _)) = trait_bound_head(&trait_ty.inner) {
                                    if let Some(&trait_def) = self.type_defs.get(&trait_symbol) {
                                        self.check_copyable_drop_conflict(
                                            type_def,
                                            trait_def,
                                            trait_ty.span,
                                        );
                                    }
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
                                        implementer,
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
                                if let Some(fn_def) = self.fn_def_for(m) {
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
                                            implementer,
                                            vec![],
                                            &trait_ty.inner,
                                            &impl_type_defs,
                                        ) {
                                            self.program_layout
                                                .trait_methods
                                                .insert((inst_key, m.name.symbol), fn_def);
                                        }
                                    } else if let TraitImplementer::Type(type_def) = implementer {
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

    /// Collects a function's type signature into `value_types` using the current generic scope.
    pub(in crate::typeck::check) fn collect_fn_sig(&mut self, f: &Function) {
        let fn_ty = self.fn_type_for_function(f);
        if let Some(def) = self.fn_def_for(f) {
            self.value_types.insert(def, fn_ty);
            if f.unsafe_ {
                self.mark_fn_effective_unsafe(def);
            }
        }
    }

    pub(in crate::typeck::check) fn collect_fn_sig_only_with_defs(
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

    pub(in crate::typeck::check) fn collect_extern_sig(
        &mut self,
        sig: &phx_syntax::ast::decl::FunctionSig,
    ) {
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

    pub(in crate::typeck::check) fn trait_subst_for_inst(
        &mut self,
        trait_def: DefId,
        inst_key: &crate::typeck::layout::TraitInstKey,
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
            self.resolved,
            trait_module,
            Some(&generics),
        );
        let param_defs = self.generic_param_defs_from_ast(trait_module, &generics);
        for (param, arg) in param_defs.iter().zip(&inst_key.trait_args) {
            trait_subst.insert(*param, *arg);
        }
        trait_subst
    }

    pub(in crate::typeck::check) fn register_inherited_trait_method_types(
        &mut self,
        trait_def: DefId,
        inst_key: &crate::typeck::layout::TraitInstKey,
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
                    .map(|p| Substitution::apply(&mut self.types, *p, &trait_subst, self.resolved))
                    .collect();
                let ret = Substitution::apply(&mut self.types, ret, &trait_subst, self.resolved);
                fn_ty = self.types.intern(&Ty::Fn { params, ret });
            }
            self.value_types.insert(*fn_def, fn_ty);
        }
    }

    pub(in crate::typeck::check) fn check_inherited_trait_methods(
        &mut self,
        inst_key: &crate::typeck::layout::TraitInstKey,
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

    /// Second-phase dispatch for one [`TopLevelItem`] after [`Self::collect_decls`].
    ///
    /// Checks function bodies, const initializers, and delegates impl blocks to [`super::impls`].
    pub(in crate::typeck::check) fn check_top_level(&mut self, item: &TopLevelItem, span: Span) {
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
}

/// Returns trait/type bound clauses for generic parameter `name` on `decl`, if any.
pub(in crate::typeck::check) fn generic_bounds_in_decl(
    decl: &TopLevelDecl,
    name: Symbol,
) -> Option<Vec<Node<Type>>> {
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

/// Returns the expression that supplies a block's trailing value, if any.
///
/// Walks items from the end: a trailing expression statement, `return` operand, or `let`/`const`
/// initializer. Assignment expressions are skipped. Used by [`super::stmt::TypeChecker::check_block_value`]
/// to avoid linting intentionally discarded tail expressions.
pub(in crate::typeck::check) fn trailing_value_expr(block: &Block) -> Option<&ExprNode> {
    for item in block.items.iter().rev() {
        match item {
            BlockItem::Expr(expr) => return Some(expr),
            BlockItem::Stmt(stmt) => match &stmt.inner {
                Stmt::Return(expr) => return expr.as_ref(),
                Stmt::Const { init, .. } | Stmt::Var { init, .. } => return Some(init),
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

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

use std::collections::{HashMap, HashSet};

use phx_diagnostics::{InvalidMainReason, ResolveError, Span};
use phx_syntax::ast::Type;
use phx_syntax::ast::decl::{
    Function, FunctionSig, ImplMember, Param, StructBody, TopLevelDecl, TopLevelItem, TraitItem,
    Variant,
};
use phx_syntax::ast::expr::{Expr, PostfixOp, StructFieldInit};
use phx_syntax::ast::ident::{Ident, Path, PathSegment, TypeName};
use phx_syntax::ast::pat::{MatchArm, Pattern};
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};
use phx_syntax::ast::types::GenericParam;
use phx_syntax::ast::{BlockNode, ExprNode, Node, PatternNode};
use phx_syntax::{Symbol, closure_def_symbol, impl_receiver_symbol};

use super::Resolver;
use super::def_id::{DefId, DefKind};

impl Resolver<'_> {
    /// Resolves the whole program in `self.source`.
    /// Runs the full resolve pass on [`Resolver::source`].
    pub(crate) fn resolve_program(&mut self) {
        if !self.allow_imports {
            for import in &self.source.program.imports {
                self.bag.push(
                    self.current_module,
                    ResolveError::ImportNotSupported { span: import.span },
                );
            }
        }

        self.scopes.push();
        for &(sym, id, is_type, bind_span) in &self.import_bindings {
            if is_type {
                self.scopes.define_type(
                    &self.defs,
                    &mut self.bag,
                    self.current_module,
                    sym,
                    id,
                    bind_span,
                );
            } else {
                self.scopes.define_value(
                    &self.defs,
                    &mut self.bag,
                    self.current_module,
                    sym,
                    id,
                    bind_span,
                );
            }
        }
        if self.collect_only {
            self.collect_top_level_defs();
        } else {
            if self.defs.is_empty() {
                self.collect_top_level_defs();
            } else {
                self.seed_module_scopes();
            }
            self.resolve_top_level_items();
            if !self.allow_imports {
                self.check_main();
            }
        }
        self.scopes.pop();
    }

    fn collect_top_level_defs(&mut self) {
        for item in &self.source.program.items {
            self.collect_top_level_item(&item.inner, item.span);
        }
    }

    fn collect_top_level_item(&mut self, item: &TopLevelItem, span: Span) {
        let exported = item.pub_;
        let item_attrs = crate::attrs::item_attrs_for_top_level(&self.source.interner, item);
        match &item.decl {
            TopLevelDecl::Struct { name, generics, .. } => {
                self.collect_generic_params(generics);
                let id = self.define_exported(
                    name.symbol,
                    name_span_type(name),
                    DefKind::Struct,
                    exported,
                );
                self.record_def_attrs(id, item_attrs);
            }
            TopLevelDecl::Enum {
                name,
                generics,
                variants,
                ..
            } => {
                self.collect_generic_params(generics);
                let id = self.define_exported(name.symbol, span, DefKind::Enum, exported);
                self.record_def_attrs(id, item_attrs.clone());
                for v in variants {
                    let vspan = name_span_type(&v.name);
                    self.define_exported(v.name.symbol, vspan, DefKind::EnumVariant, exported);
                }
            }
            TopLevelDecl::TypeAlias { name, generics, .. } => {
                self.collect_generic_params(generics);
                let id = self.define_exported(name.symbol, span, DefKind::TypeAlias, exported);
                self.record_def_attrs(id, item_attrs);
            }
            TopLevelDecl::Trait { name, generics, .. } => {
                self.collect_generic_params(generics);
                let id = self.define_exported(name.symbol, span, DefKind::Trait, exported);
                self.record_def_attrs(id, item_attrs);
            }
            TopLevelDecl::Impl {
                generics, members, ..
            } => {
                self.collect_generic_params(generics);
                for member in members {
                    if let ImplMember::Method(f) = member {
                        let mspan = name_span_ident(&f.name);
                        let id = self.define_value(f.name.symbol, mspan, DefKind::Fn);
                        let attrs = crate::attrs::item_attrs_for_function(&self.source.interner, f);
                        self.record_def_attrs(id, attrs);
                    }
                }
            }
            TopLevelDecl::Function(f) => {
                self.collect_generic_params(&f.generics);
                let id = self.define_exported(f.name.symbol, span, DefKind::Fn, exported);
                self.record_def_attrs(id, item_attrs);
                if self.is_main_name(f.name.symbol) {
                    if self.current_module == self.root_module {
                        self.main_fn = Some(id);
                    } else {
                        self.bag.push(
                            self.current_module,
                            ResolveError::MainNotInEntry {
                                span,
                                module: self.logical_path.to_owned(),
                            },
                        );
                    }
                }
            }
            TopLevelDecl::Const { name, .. } => {
                let id = self.define_exported(name.symbol, span, DefKind::Const, exported);
                self.record_def_attrs(id, item_attrs);
            }
            TopLevelDecl::Var { name, .. } => {
                let id = self.define_exported(name.symbol, span, DefKind::Var, exported);
                self.record_def_attrs(id, item_attrs);
            }
            TopLevelDecl::Mod { .. } => {}
            TopLevelDecl::Reexport { .. } => {
                if !exported {
                    self.bag.push(
                        self.current_module,
                        ResolveError::ReexportRequiresPub { span },
                    );
                }
            }
            TopLevelDecl::ExternBlock { items, .. } => {
                self.collect_extern_fns(items, exported, &item_attrs);
            }
            TopLevelDecl::ExternItem { sig, .. } => {
                self.collect_extern_fns(std::slice::from_ref(sig), exported, &item_attrs);
            }
            _ => {}
        }
    }

    fn collect_extern_fns(
        &mut self,
        sigs: &[FunctionSig],
        exported: bool,
        item_attrs: &crate::attrs::ItemAttrs,
    ) {
        for sig in sigs {
            let sig_span = name_span_ident(&sig.name);
            let id = self.define_exported(sig.name.symbol, sig_span, DefKind::ExternFn, exported);
            self.record_def_attrs(id, item_attrs.clone());
        }
    }

    /// Registers existing program defs for this module into scope (phase-2 resolve).
    fn seed_module_scopes(&mut self) {
        for (i, def) in self.defs.iter().enumerate() {
            if def.module != self.current_module {
                continue;
            }
            let id = DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX));
            match def.kind {
                DefKind::Struct
                | DefKind::Enum
                | DefKind::TypeAlias
                | DefKind::Trait
                | DefKind::GenericParam => {
                    self.scopes.define_type(
                        &self.defs,
                        &mut self.bag,
                        self.current_module,
                        def.name,
                        id,
                        def.span,
                    );
                }
                DefKind::Fn
                | DefKind::Const
                | DefKind::Var
                | DefKind::EnumVariant
                | DefKind::ExternFn => {
                    self.scopes.define_value(
                        &self.defs,
                        &mut self.bag,
                        self.current_module,
                        def.name,
                        id,
                        def.span,
                    );
                }
                DefKind::StructField
                | DefKind::Param
                | DefKind::Local
                | DefKind::Impl
                | DefKind::Closure
                | DefKind::TraitAssocType => {}
            }
        }
    }

    fn resolve_top_level_items(&mut self) {
        for item in &self.source.program.items {
            self.resolve_top_level_decl(&item.inner.decl, item.span);
        }
    }

    fn register_trait_impl(
        &mut self,
        type_name: &TypeName,
        trait_: &Option<Node<Type>>,
        span: Span,
    ) {
        let Some(trait_ty) = trait_ else {
            return;
        };
        let key = (type_name.symbol, trait_ty.inner.clone());
        for &(ty, ref tr, first_span) in &self.trait_impls {
            if ty == key.0 && tr.as_ref().is_some_and(|tr| trait_types_equal(tr, &key.1)) {
                self.bag.push(
                    self.current_module,
                    ResolveError::DuplicateTraitImpl { span, first_span },
                );
                return;
            }
        }
        self.trait_impls.push((key.0, Some(key.1), span));
    }

    fn resolve_top_level_decl(&mut self, decl: &TopLevelDecl, item_span: Span) {
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
                self.self_type_depth += 1;
                for item in items {
                    self.resolve_trait_item(item);
                }
                self.self_type_depth -= 1;
                self.scopes.pop();
            }
            TopLevelDecl::Impl {
                type_name,
                generics,
                trait_,
                members,
                ..
            } => {
                self.register_trait_impl(type_name, trait_, item_span);
                self.scopes.push();
                self.resolve_generics(generics);
                self.self_type_depth += 1;
                for member in members {
                    match member {
                        ImplMember::AssociatedType { ty, .. } => self.resolve_type_node(ty),
                        ImplMember::Method(f) => self.resolve_function(f, true),
                        _ => {}
                    }
                }
                self.self_type_depth -= 1;
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
            TopLevelDecl::ExternBlock { items, .. } => {
                for sig in items {
                    self.resolve_extern_sig(sig);
                }
            }
            TopLevelDecl::ExternItem { sig, .. } => {
                self.resolve_extern_sig(sig);
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

    fn collect_generic_params(&mut self, generics: &Option<Vec<GenericParam>>) {
        if !self.collect_only {
            return;
        }
        if let Some(params) = generics {
            for param in params {
                let span = name_span_ident(&param.name);
                if self.existing_generic_param_def(param.name.symbol).is_some() {
                    continue;
                }
                if let Some(id) = self.existing_generic_param_def(param.name.symbol) {
                    self.scopes.define_type(
                        &self.defs,
                        &mut self.bag,
                        self.current_module,
                        param.name.symbol,
                        id,
                        span,
                    );
                } else {
                    self.define_type(param.name.symbol, span, DefKind::GenericParam);
                }
            }
        }
    }

    fn existing_generic_param_def(&self, name: Symbol) -> Option<DefId> {
        self.defs.iter().enumerate().find_map(|(i, d)| {
            if d.module == self.current_module && d.name == name && d.kind == DefKind::GenericParam
            {
                Some(DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)))
            } else {
                None
            }
        })
    }

    fn resolve_generics(&mut self, generics: &Option<Vec<GenericParam>>) {
        if let Some(params) = generics {
            let mut seen: HashMap<Symbol, Span> = HashMap::new();
            for param in params {
                let span = name_span_ident(&param.name);
                if let Some(first_span) = seen.insert(param.name.symbol, span) {
                    self.bag.push(
                        self.current_module,
                        ResolveError::DuplicateDefinition {
                            symbol_index: param.name.symbol.index(),
                            first_span,
                            span,
                        },
                    );
                    continue;
                }
                self.define_type(param.name.symbol, span, DefKind::GenericParam);
                if let Some(bounds) = &param.bounds {
                    for bound in bounds {
                        self.resolve_trait_bound(bound);
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

    fn resolve_extern_sig(&mut self, sig: &FunctionSig) {
        self.scopes.push();
        self.resolve_params(&sig.params);
        if let Some(ret) = &sig.ret {
            self.resolve_type_node(ret);
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
            BlockItem::Import(imp) => self.apply_block_import(imp),
            _ => {}
        }
    }

    fn apply_block_import(
        &mut self,
        imp: &phx_syntax::ast::Node<phx_syntax::ast::decl::ImportDirective>,
    ) {
        let Some(env) = self.import_env else {
            return;
        };
        let Some(interner) = self.shared_interner.as_deref_mut() else {
            return;
        };
        let Some(import_types) = self.import_types.as_deref_mut() else {
            return;
        };
        let Some(module) = env
            .modules
            .iter()
            .find(|m| m.id.index() == self.current_module)
        else {
            return;
        };

        let mut seen = HashSet::new();
        let mut ctx = crate::modules::import_resolve::ImportResolveCtx {
            module,
            modules: env.modules,
            path_index: env.path_index,
            exports: env.exports,
            defs: env.defs,
            layout: env.layout,
            workspace_name: env.workspace_name,
            dep_names: env.dep_names,
            interner,
            import_types,
            bag: &mut self.bag,
            submodules: env.submodules,
        };
        let bindings = crate::modules::import_resolve::resolve_import_directive(
            &imp.inner, imp.span, &mut ctx, &mut seen,
        );
        for &(sym, id, is_type, bind_span) in &bindings {
            if is_type {
                self.scopes.define_type(
                    &self.defs,
                    &mut self.bag,
                    self.current_module,
                    sym,
                    id,
                    bind_span,
                );
            } else {
                self.scopes.define_value(
                    &self.defs,
                    &mut self.bag,
                    self.current_module,
                    sym,
                    id,
                    bind_span,
                );
            }
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
            Stmt::Break { value, .. } => {
                if let Some(e) = value {
                    self.resolve_expr_node(e);
                }
            }
            Stmt::Continue { .. } => {}
            Stmt::While { cond, body } => {
                self.resolve_expr_node(cond);
                self.resolve_block_node(body);
            }
            Stmt::ForIn {
                binding,
                iter,
                body,
            } => {
                self.define_value(binding.symbol, name_span_ident(binding), DefKind::Local);
                self.resolve_expr_node(iter);
                self.resolve_block_node(body);
            }
            Stmt::Loop(body) => self.resolve_block_node(body),
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
                self.resolve_type_name(name);
                let _ = span;
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
            Type::SelfAssoc { member } => self.resolve_type_name(member),
            _ => {}
        }
    }

    fn resolve_type_name(&mut self, name: &TypeName) {
        if self.is_self_type_name(name) && self.self_type_depth > 0 {
            return;
        }
        let def_id = self.scopes.lookup_type(name.symbol);
        if def_id.is_some() {
            self.record_resolution(name.id, def_id);
        } else {
            self.bag.push(
                self.current_module,
                ResolveError::UnresolvedType {
                    symbol_index: name.symbol.index(),
                    span: name.span,
                },
            );
        }
    }

    fn resolve_trait_bound(&mut self, bound: &Node<Type>) {
        if let Type::Named { name, .. } = &bound.inner {
            if self.is_bootstrap_trait_bound(name.symbol) {
                return;
            }
        }
        self.resolve_type_node(bound);
    }

    fn is_self_type_name(&self, name: &TypeName) -> bool {
        self.source.interner.resolve(name.symbol) == "Self"
    }

    fn is_bootstrap_trait_bound(&self, trait_symbol: Symbol) -> bool {
        self.source.interner.resolve(trait_symbol) == "Copyable"
    }

    /// Resolves a `PascalCase` name in expression position (enum variant ctors before types).
    fn resolve_type_or_value_name(&mut self, name: &TypeName, expr_span: Span) {
        if let Some(id) = self.scopes.lookup_value(name.symbol) {
            self.record_resolution(name.id, Some(id));
            return;
        }
        self.resolve_type_name(name);
        let _ = expr_span;
    }

    fn resolve_expr_node(&mut self, expr: &ExprNode) {
        self.resolve_expr(&expr.inner, expr.id, expr.span);
    }

    #[allow(clippy::too_many_lines)]
    fn resolve_expr(&mut self, expr: &Expr, node_id: phx_syntax::AstNodeId, span: Span) {
        match expr {
            Expr::Literal(_) => {}
            Expr::Ident(ident) => self.resolve_ident(ident),
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
                condition,
                then_block,
                else_ifs,
                else_block,
            } => {
                self.resolve_if_condition(condition.as_ref());
                self.scopes.push();
                self.resolve_if_pattern_bindings(condition.as_ref());
                self.resolve_block_node(then_block);
                self.scopes.pop();
                for (c, b) in else_ifs {
                    self.resolve_if_condition(c);
                    self.scopes.push();
                    self.resolve_if_pattern_bindings(c);
                    self.resolve_block_node(b);
                    self.scopes.pop();
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
            Expr::Range { start, end, .. } => {
                self.resolve_expr_node(start);
                self.resolve_expr_node(end);
            }
            Expr::Lambda { params, body } => {
                let closure_id =
                    self.alloc_def(DefKind::Closure, closure_def_symbol(), span, false);
                self.closure_stack.push(closure_id);
                self.scopes.push();
                for p in params {
                    match p {
                        Param::Named { name, ty, .. } => {
                            self.define_value(name.symbol, name_span_ident(name), DefKind::Param);
                            self.resolve_type_node(ty);
                        }
                        Param::Receiver { ty, .. } => {
                            self.define_value(
                                impl_receiver_symbol(),
                                Span::new(0, 0),
                                DefKind::Param,
                            );
                            if let Some(t) = ty {
                                self.resolve_type_node(t);
                            }
                        }
                        _ => {}
                    }
                }
                match body {
                    phx_syntax::ast::expr::LambdaBody::Expr(e) => self.resolve_expr_node(e),
                    phx_syntax::ast::expr::LambdaBody::Block(b) => self.resolve_block_node(b),
                    _ => {}
                }
                self.scopes.pop();
                self.closure_stack.pop();
                let _ = node_id;
            }
            Expr::RuntimeDirective { args, .. } => {
                for arg in args {
                    self.resolve_expr_node(arg);
                }
            }
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
            PostfixOp::Call { args, .. } => {
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

    fn resolve_if_condition(&mut self, condition: &phx_syntax::ast::expr::IfCondition) {
        match condition {
            phx_syntax::ast::expr::IfCondition::Bool(cond) => self.resolve_expr_node(cond),
            phx_syntax::ast::expr::IfCondition::Pattern { scrutinee, .. } => {
                self.resolve_expr_node(scrutinee);
            }
        }
    }

    fn resolve_if_pattern_bindings(&mut self, condition: &phx_syntax::ast::expr::IfCondition) {
        if let phx_syntax::ast::expr::IfCondition::Pattern { pattern, .. } = condition {
            self.resolve_pattern_node(pattern);
        }
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

    fn resolve_ident(&mut self, ident: &Ident) {
        if let Some(id) = self.scopes.lookup_value(ident.symbol) {
            if self
                .defs
                .get(id.index() as usize)
                .is_some_and(|d| matches!(d.kind, DefKind::GenericParam))
            {
                self.push_generic_param_in_value(ident);
                return;
            }
            self.record_resolution(ident.id, Some(id));
            return;
        }
        if let Some(id) = self.scopes.lookup_type(ident.symbol) {
            if self
                .defs
                .get(id.index() as usize)
                .is_some_and(|d| matches!(d.kind, DefKind::GenericParam))
            {
                self.push_generic_param_in_value(ident);
                return;
            }
        }
        self.bag.push(
            self.current_module,
            ResolveError::UnresolvedIdent {
                symbol_index: ident.symbol.index(),
                span: ident.span,
            },
        );
    }

    fn push_generic_param_in_value(&mut self, ident: &Ident) {
        self.bag.push(
            self.current_module,
            ResolveError::GenericParamInValue {
                symbol_index: ident.symbol.index(),
                span: ident.span,
            },
        );
    }

    fn resolve_path_expr(&mut self, path: &Path, span: Span) {
        if path.segments.is_empty() {
            return;
        }
        if path.segments.len() >= 2 && matches!(path.segments[1], PathSegment::Ident(_)) {
            match &path.segments[0] {
                PathSegment::Type(name) => self.resolve_type_or_value_name(name, span),
                PathSegment::Ident(ident) => self.resolve_type_param_in_assoc_path(ident),
            }
            for seg in path.segments.iter().skip(2) {
                match seg {
                    PathSegment::Ident(ident) => self.resolve_ident(ident),
                    PathSegment::Type(name) => self.resolve_type_name(name),
                }
            }
            return;
        }
        match &path.segments[0] {
            PathSegment::Ident(ident) => self.resolve_ident(ident),
            PathSegment::Type(name) => {
                self.resolve_type_or_value_name(name, span);
            }
        }
        if path.segments.len() > 1 {
            for seg in &path.segments[1..] {
                match seg {
                    PathSegment::Ident(ident) => self.resolve_ident(ident),
                    PathSegment::Type(name) => self.resolve_type_name(name),
                }
            }
        } else {
            let _ = span;
        }
    }

    fn resolve_type_param_in_assoc_path(&mut self, ident: &Ident) {
        if let Some(id) = self
            .scopes
            .lookup_type(ident.symbol)
            .or_else(|| self.scopes.lookup_value(ident.symbol))
        {
            if self
                .defs
                .get(id.index() as usize)
                .is_some_and(|d| matches!(d.kind, DefKind::GenericParam))
            {
                self.record_resolution(ident.id, Some(id));
                return;
            }
        }
        self.resolve_ident(ident);
    }

    /// Validates MVP entry `main :: () => { … }`.
    pub(crate) fn check_main(&mut self) {
        if self.main_fn.is_none() {
            self.bag.push(
                self.current_module,
                ResolveError::MissingMain {
                    span: self
                        .main_decl_name_span()
                        .unwrap_or_else(|| self.program_hint_span()),
                },
            );
            return;
        }

        let mut has_params = false;
        let mut params_span = None;
        let mut bad_ret_span = None;

        for item in &self.source.program.items {
            let TopLevelDecl::Function(f) = &item.inner.decl else {
                continue;
            };
            if !self.is_main_name(f.name.symbol) {
                continue;
            }
            has_params = !f.params.is_empty();
            params_span = Some(f.name.span);
            if let Some(ret) = &f.ret {
                if !type_is_unit(&ret.inner) {
                    bad_ret_span = Some(ret.span);
                }
            }
            break;
        }

        if has_params {
            self.bag.push(
                self.current_module,
                ResolveError::InvalidMainSignature {
                    span: params_span.unwrap_or_else(|| self.program_hint_span()),
                    reason: InvalidMainReason::HasParameters,
                },
            );
        }
        if let Some(span) = bad_ret_span {
            self.bag.push(
                self.current_module,
                ResolveError::InvalidMainSignature {
                    span,
                    reason: InvalidMainReason::NonUnitReturn,
                },
            );
        }
    }

    /// Returns `true` when `symbol` is the interned `main` identifier.
    pub(crate) fn is_main_name(&self, symbol: Symbol) -> bool {
        self.source.interner.resolve(symbol) == "main"
    }

    fn program_hint_span(&self) -> Span {
        if let Some(item) = self.source.program.items.first() {
            item.span
        } else if let Some(imp) = self.source.program.imports.first() {
            imp.span
        } else {
            Span::new(0, 1)
        }
    }

    fn main_decl_name_span(&self) -> Option<Span> {
        for item in &self.source.program.items {
            let TopLevelDecl::Function(f) = &item.inner.decl else {
                continue;
            };
            if self.is_main_name(f.name.symbol) {
                return Some(f.name.span);
            }
        }
        None
    }
}

fn type_is_unit(ty: &Type) -> bool {
    matches!(ty, Type::Unit)
}

fn trait_types_equal(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (
            Type::Named {
                name: na,
                generics: ga,
            },
            Type::Named {
                name: nb,
                generics: gb,
            },
        ) => {
            if na.symbol != nb.symbol {
                return false;
            }
            match (ga, gb) {
                (None, None) => true,
                (Some(a_args), Some(b_args)) => {
                    a_args.len() == b_args.len()
                        && a_args
                            .iter()
                            .zip(b_args)
                            .all(|(a, b)| trait_types_equal(&a.inner, &b.inner))
                }
                _ => false,
            }
        }
        (Type::Primitive(ka), Type::Primitive(kb)) => ka == kb,
        (Type::Unit, Type::Unit) => true,
        _ => false,
    }
}

fn name_span_ident(ident: &Ident) -> Span {
    ident.span
}

fn name_span_type(name: &TypeName) -> Span {
    name.span
}

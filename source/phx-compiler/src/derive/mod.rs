//! `#[derive(...)]` expansion before name resolution (V0-056).
//!
//! Runs immediately after parsing and attribute merging, before the resolver walk.
//! Each `#[derive(Copyable, PartialEq, Debug)]` on a struct or enum is replaced by
//! synthetic `impl` items appended after the type declaration; the original derive
//! list on the type is cleared so later passes see ordinary impl blocks only.
//!
//! ## Supported traits
//!
//! | Trait | Generated impl |
//! | ----- | -------------- |
//! | `Copyable` | Marker impl with generic bounds on type parameters used in fields |
//! | `PartialEq` | `eq(&self, other: &Self) -> bool` — field-wise for structs, tag + payload match for enums |
//! | `Debug` | `fmt(&self) -> [u8; 32]` — type name bytes, zero-padded (MVP debug formatting) |
//!
//! Duplicate derives, unsupported trait names, or a type that already implements the
//! trait (explicit impl in the same module) produce [`DeriveError`].
//!
//! ## Generic bounds
//!
//! For `Copyable` and `PartialEq`, any generic type parameter referenced by a field
//! or variant payload receives a bound to the derived trait (unless already present).
//! `Debug` does not propagate bounds — its generated method ignores field values.
//!
//! ## Pipeline position
//!
//! Called from [`crate::modules::loader`] after [`crate::attrs::merge_bracket_derives_into_program`]
//! merges bracket-style `#[derive(...)]` attributes into [`DeriveDirective`] lists on each type.

use std::collections::HashSet;

use phx_diagnostics::Span;
use phx_syntax::ast::decl::{
    DeriveDirective, EnumVariant, Function, ImplMember, Param, StructBody, TopLevelDecl,
    TopLevelItem, Variant,
};
use phx_syntax::ast::expr::{BinOp, Expr, IfCondition, PostfixOp, UnaryOp};
use phx_syntax::ast::ident::{Ident, TypeName};
use phx_syntax::ast::lit::{IntLit, Literal};
use phx_syntax::ast::pat::MatchArm;
use phx_syntax::ast::pat::{Pattern, StructPatternField};
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};
use phx_syntax::ast::types::{GenericParam, Type};
use phx_syntax::ast::{AstNodeId, BlockNode, ExprNode, Node, PatternNode};
use phx_syntax::token::{IntegerSuffix, Keyword};
use phx_syntax::{InternError, Interner, Program, Symbol, impl_receiver_symbol};

/// Failure while expanding `#[derive(...)]`.
///
/// Returned by [`expand_derives`] when a derive list is invalid or conflicts with an
/// existing impl in the same compilation unit. The span points at the offending trait
/// name or the type's derive site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeriveError {
    /// Related source span.
    pub span: Span,
    /// Human-readable message.
    pub message: String,
}

const SUPPORTED_DERIVES: &[&str] = &["Copyable", "PartialEq", "Debug"];

/// Supported compile-time derive traits (V0-056).
///
/// Parsed from interned trait names in `#[derive(...)]` attribute lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeriveTrait {
    /// Marker trait for bitwise-copyable types; adds bounds on generic parameters in fields.
    Copyable,
    /// Structural equality; generates `eq` comparing fields or enum tags and payloads.
    PartialEq,
    /// MVP debug formatting; generates `fmt` returning the type name as a fixed `[u8; 32]`.
    Debug,
}

impl DeriveTrait {
    fn parse(interner: &Interner, name: Symbol) -> Option<Self> {
        match interner.resolve(name) {
            Some("Copyable") => Some(Self::Copyable),
            Some("PartialEq") => Some(Self::PartialEq),
            Some("Debug") => Some(Self::Debug),
            _ => None,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Copyable => "Copyable",
            Self::PartialEq => "PartialEq",
            Self::Debug => "Debug",
        }
    }
}

/// Expands `#[derive(...)]` on structs and enums into synthetic trait impl items.
///
/// Walks [`Program::items`](phx_syntax::Program::items) in order. For each struct or
/// enum with a non-empty derive list, appends generated impl blocks after the type
/// and clears the derive list on the type node. Other top-level items pass through
/// unchanged.
///
/// Existing explicit impls in the same program are collected first so duplicate
/// derive requests are rejected before any AST is synthesized.
///
/// # Errors
///
/// Returns [`DeriveError`] when:
///
/// - a trait name is not one of `Copyable`, `PartialEq`, or `Debug`;
/// - the same trait appears twice in a type's derive list;
/// - the type already has an explicit impl for that trait in this module;
/// - the identifier intern table fills while synthesizing AST nodes.
pub fn expand_derives(program: &mut Program, interner: &mut Interner) -> Result<(), DeriveError> {
    crate::attrs::merge_bracket_derives_into_program(program, interner);
    let existing = collect_existing_impls(&program.items, interner);
    let taken = std::mem::take(&mut program.items);
    let mut out = Vec::with_capacity(taken.len());
    for mut item in taken {
        let extra = match &mut item.inner.decl {
            TopLevelDecl::Struct {
                name,
                derives,
                generics,
                body,
            } if !derives.is_empty() => {
                let span = item.span;
                let expanded = expand_type_derives(
                    interner,
                    &existing,
                    name,
                    generics.as_deref(),
                    TypeShape::Struct(body),
                    derives,
                    span,
                )?;
                *derives = Vec::new();
                expanded
            }
            TopLevelDecl::Enum {
                name,
                derives,
                generics,
                variants,
            } if !derives.is_empty() => {
                let span = item.span;
                let expanded = expand_type_derives(
                    interner,
                    &existing,
                    name,
                    generics.as_deref(),
                    TypeShape::Enum(variants),
                    derives,
                    span,
                )?;
                *derives = Vec::new();
                expanded
            }
            _ => Vec::new(),
        };
        out.push(item);
        out.extend(extra);
    }
    program.items = out;
    Ok(())
}

/// Struct or enum body shape used when synthesizing `PartialEq` and generic bounds.
#[derive(Clone, Copy)]
enum TypeShape<'a> {
    /// Named-field, tuple, or unit struct body.
    Struct(&'a StructBody),
    /// Enum variant list.
    Enum(&'a [EnumVariant]),
}

/// Collects `(type_symbol, trait_symbol)` pairs for explicit impl blocks already in `items`.
///
/// Used to reject derives that would duplicate a user-written impl in the same module.
fn collect_existing_impls(
    items: &[Node<TopLevelItem>],
    interner: &Interner,
) -> Vec<(Symbol, Symbol)> {
    let mut out = Vec::new();
    for item in items {
        if let TopLevelDecl::Impl {
            type_name, trait_, ..
        } = &item.inner.decl
            && let Some(trait_sym) = trait_symbol(trait_.as_ref())
        {
            out.push((type_name.symbol, trait_sym));
        }
    }
    let _ = interner;
    out
}

/// Extracts the trait name symbol from an impl's optional trait type node.
fn trait_symbol(trait_: Option<&Node<Type>>) -> Option<Symbol> {
    match trait_.map(|t| &t.inner) {
        Some(Type::Named { name, .. }) => Some(name.symbol),
        _ => None,
    }
}

/// Records a generic type parameter referenced bare (no arguments) inside `ty`.
fn visit_type_param_refs(ty: &Type, param_syms: &HashSet<Symbol>, required: &mut HashSet<Symbol>) {
    if let Type::Named {
        name,
        generics: None,
    } = ty
        && param_syms.contains(&name.symbol)
    {
        required.insert(name.symbol);
    }
}

/// Generic parameters that must receive a bound for the given derived trait.
///
/// Scans struct fields or enum variant payloads for bare uses of a generic parameter
/// name (e.g. `T` in `field: T`). Empty when the type has no generic parameter list.
fn required_param_bounds(
    shape: TypeShape<'_>,
    generic_params: Option<&[GenericParam]>,
) -> HashSet<Symbol> {
    let Some(params) = generic_params.filter(|p| !p.is_empty()) else {
        return HashSet::new();
    };
    let param_syms: HashSet<Symbol> = params.iter().map(|p| p.name.symbol).collect();
    let mut required = HashSet::new();
    match shape {
        TypeShape::Struct(body) => match body {
            StructBody::Fields(fields) => {
                for field in fields {
                    visit_type_param_refs(&field.ty.inner, &param_syms, &mut required);
                }
            }
            StructBody::Tuple(types) => {
                for ty in types {
                    visit_type_param_refs(&ty.inner, &param_syms, &mut required);
                }
            }
            StructBody::Unit => {}
        },
        TypeShape::Enum(variants) => {
            for variant in variants {
                match &variant.kind {
                    Variant::Unit => {}
                    Variant::Tuple(types) => {
                        for ty in types {
                            visit_type_param_refs(&ty.inner, &param_syms, &mut required);
                        }
                    }
                    Variant::Struct(fields) => {
                        for field in fields {
                            visit_type_param_refs(&field.ty.inner, &param_syms, &mut required);
                        }
                    }
                }
            }
        }
    }
    required
}

/// Validates derive directives and builds synthetic impl items for one type.
///
/// Checks for unsupported traits, duplicates within the derive list, and conflicts
/// with `existing` impls or impls already planned in this expansion batch.
fn expand_type_derives(
    interner: &mut Interner,
    existing: &[(Symbol, Symbol)],
    type_name: &TypeName,
    generics: Option<&[GenericParam]>,
    shape: TypeShape<'_>,
    derives: &[DeriveDirective],
    span: Span,
) -> Result<Vec<Node<TopLevelItem>>, DeriveError> {
    let mut seen = Vec::new();
    let mut planned = Vec::new();
    for dir in derives {
        for trait_name in &dir.traits {
            let Some(kind) = DeriveTrait::parse(interner, trait_name.symbol) else {
                let supported = SUPPORTED_DERIVES.join(", ");
                let name = interner.resolve(trait_name.symbol).unwrap_or("<?>");
                return Err(DeriveError {
                    span: trait_name.span,
                    message: format!("unsupported derive trait `{name}`; supported: {supported}"),
                });
            };
            if seen.contains(&kind) {
                let type_display = interner.resolve_display(type_name.symbol);
                return Err(DeriveError {
                    span: trait_name.span,
                    message: format!("duplicate derive `{}` for `{type_display}`", kind.as_str()),
                });
            }
            if existing
                .iter()
                .any(|(t, tr)| *t == type_name.symbol && interner.resolves_to(*tr, kind.as_str()))
            {
                let type_display = interner.resolve_display(type_name.symbol);
                return Err(DeriveError {
                    span: trait_name.span,
                    message: format!(
                        "cannot derive {}: `{type_display}` already implements {}",
                        kind.as_str(),
                        kind.as_str()
                    ),
                });
            }
            seen.push(kind);
            planned.push(kind);
        }
    }
    let mut builder = AstGen::new(interner, span);
    let mut impls = Vec::with_capacity(planned.len());
    for kind in planned {
        if impls
            .iter()
            .any(|item| impl_trait_kind(item, builder.interner) == Some(kind))
        {
            let type_display = builder.interner.resolve_display(type_name.symbol);
            return Err(DeriveError {
                span,
                message: format!(
                    "cannot derive {}: `{type_display}` already implements {}",
                    kind.as_str(),
                    kind.as_str()
                ),
            });
        }
        let required = required_param_bounds(shape, generics);
        let item = match kind {
            DeriveTrait::Copyable => builder.copyable_impl(type_name, generics, &required)?,
            DeriveTrait::PartialEq => {
                builder.partialeq_impl(type_name, shape, generics, &required)?
            }
            DeriveTrait::Debug => builder.debug_impl(type_name, generics)?,
        };
        impls.push(item);
    }
    Ok(impls)
}

/// Maps a synthesized impl item back to its [`DeriveTrait`] kind, if any.
fn impl_trait_kind(item: &Node<TopLevelItem>, interner: &Interner) -> Option<DeriveTrait> {
    let TopLevelDecl::Impl { trait_, .. } = &item.inner.decl else {
        return None;
    };
    trait_symbol(trait_.as_ref()).and_then(|s| DeriveTrait::parse(interner, s))
}

/// Allocates synthetic AST nodes for a single derive expansion.
///
/// Uses a dedicated node-id range starting at `0x4_000_000` so derived nodes do not
/// collide with parse-time ids. Intern failures are deferred and surfaced by
/// [`Self::finish`].
struct AstGen<'a> {
    interner: &'a mut Interner,
    span: Span,
    next_id: u32,
    failed: Option<DeriveError>,
}

impl<'a> AstGen<'a> {
    fn new(interner: &'a mut Interner, span: Span) -> Self {
        Self {
            interner,
            span,
            next_id: 0x4_000_000,
            failed: None,
        }
    }

    fn alloc_id(&mut self) -> AstNodeId {
        let id = self.next_id;
        self.next_id = id.saturating_add(1);
        AstNodeId::from_raw(id)
    }

    fn node<T>(&mut self, inner: T) -> Node<T> {
        Node::new(inner, self.span, self.alloc_id())
    }

    fn ident(&mut self, name: &str) -> Ident {
        let symbol = self.intern_sym(name);
        Ident {
            symbol,
            span: self.span,
            id: self.alloc_id(),
        }
    }

    fn type_name(&mut self, name: &str) -> TypeName {
        let symbol = self.intern_sym(name);
        TypeName {
            symbol,
            span: self.span,
            id: self.alloc_id(),
        }
    }

    fn intern_sym(&mut self, name: &str) -> Symbol {
        if self.failed.is_some() {
            return Symbol::from_raw(0);
        }
        match self.interner.intern(name) {
            Ok(s) => s,
            Err(InternError::TableFull) => {
                self.failed = Some(DeriveError {
                    span: self.span,
                    message: "identifier intern table is full during derive expansion".to_string(),
                });
                Symbol::from_raw(0)
            }
        }
    }

    fn finish(&mut self, item: Node<TopLevelItem>) -> Result<Node<TopLevelItem>, DeriveError> {
        self.failed.take().map_or(Ok(item), Err)
    }

    fn type_bool(&mut self) -> Node<Type> {
        self.node(Type::Primitive(Keyword::Bool))
    }

    fn type_ref_self(&mut self) -> Node<Type> {
        let inner = self.type_self();
        self.node(Type::Ref {
            mut_: false,
            inner: Box::new(inner),
        })
    }

    fn type_self(&mut self) -> Node<Type> {
        let name = self.type_name("Self");
        self.node(Type::Named {
            name,
            generics: None,
        })
    }

    fn type_u8_array_32(&mut self) -> Node<Type> {
        let elem = self.node(Type::Primitive(Keyword::U8));
        self.node(Type::Array {
            elem: Box::new(elem),
            len: IntLit {
                value: 32,
                suffix: IntegerSuffix::None,
            },
        })
    }

    fn receiver_param(&mut self) -> Param {
        Param::Receiver {
            span: Span::new(0, 1),
            mut_: false,
            ty: Some(self.type_ref_self()),
        }
    }

    fn named_ref_self_param(&mut self, name: &str) -> Param {
        let param_name = self.ident(name);
        let ty = self.type_ref_self();
        Param::Named {
            name: param_name,
            ty,
        }
    }

    fn trait_type(&mut self, name: &str) -> Node<Type> {
        let trait_name = self.type_name(name);
        self.node(Type::Named {
            name: trait_name,
            generics: None,
        })
    }

    fn copyable_impl(
        &mut self,
        type_name: &TypeName,
        generic_params: Option<&[GenericParam]>,
        required: &HashSet<Symbol>,
    ) -> Result<Node<TopLevelItem>, DeriveError> {
        let impl_generics = self.impl_generics_with_bounds(generic_params, required, "Copyable");
        let item = self.trait_impl(type_name, "Copyable", Vec::new(), impl_generics);
        self.finish(item)
    }

    fn partialeq_impl(
        &mut self,
        type_name: &TypeName,
        shape: TypeShape<'_>,
        generic_params: Option<&[GenericParam]>,
        required: &HashSet<Symbol>,
    ) -> Result<Node<TopLevelItem>, DeriveError> {
        let impl_generics = self.impl_generics_with_bounds(generic_params, required, "PartialEq");
        let body = match shape {
            TypeShape::Struct(body) => self.struct_partialeq_body(type_name, body),
            TypeShape::Enum(variants) => {
                self.enum_partialeq_body(type_name, variants, generic_params)
            }
        };
        let eq_params = vec![self.receiver_param(), self.named_ref_self_param("other")];
        let eq_ret = self.type_bool();
        let method = self.trait_method("eq", eq_params, eq_ret, body);
        let item = self.trait_impl(
            type_name,
            "PartialEq",
            vec![ImplMember::Method(method)],
            impl_generics,
        );
        self.finish(item)
    }

    fn debug_impl(
        &mut self,
        type_name: &TypeName,
        generic_params: Option<&[GenericParam]>,
    ) -> Result<Node<TopLevelItem>, DeriveError> {
        let name = self.interner.resolve(type_name.symbol).unwrap_or("<?>");
        let bytes = debug_name_bytes(name);
        let array_expr = self.u8_array_literal(&bytes);
        let block = self.expr_block(array_expr);
        let fmt_params = vec![self.receiver_param()];
        let fmt_ret = self.type_u8_array_32();
        let method = self.trait_method("fmt", fmt_params, fmt_ret, block);
        let impl_generics =
            self.impl_generics_with_bounds(generic_params, &HashSet::new(), "Debug");
        let item = self.trait_impl(
            type_name,
            "Debug",
            vec![ImplMember::Method(method)],
            impl_generics,
        );
        self.finish(item)
    }

    fn impl_generics_with_bounds(
        &mut self,
        params: Option<&[GenericParam]>,
        required: &HashSet<Symbol>,
        trait_name: &str,
    ) -> Option<Vec<GenericParam>> {
        let params = params.filter(|p| !p.is_empty())?;
        let mut out = Vec::with_capacity(params.len());
        for param in params {
            let mut merged = param.clone();
            if required.contains(&param.name.symbol) {
                let bound = self.trait_type(trait_name);
                let already_bound = merged.bounds.as_ref().is_some_and(|bounds| {
                    bounds.iter().any(|b| {
                        matches!(
                            &b.inner,
                            Type::Named { name, generics: None }
                            if self.interner.resolves_to(name.symbol, trait_name)
                        )
                    })
                });
                if !already_bound {
                    match &mut merged.bounds {
                        Some(bounds) => bounds.push(bound),
                        None => merged.bounds = Some(vec![bound]),
                    }
                }
            }
            out.push(merged);
        }
        Some(out)
    }

    fn generic_type_args(&mut self, params: &[GenericParam]) -> Vec<Node<Type>> {
        params
            .iter()
            .map(|param| {
                let name = TypeName {
                    symbol: param.name.symbol,
                    span: param.name.span,
                    id: self.alloc_id(),
                };
                self.node(Type::Named {
                    name,
                    generics: None,
                })
            })
            .collect()
    }

    fn trait_impl(
        &mut self,
        type_name: &TypeName,
        trait_name: &str,
        members: Vec<ImplMember>,
        generics: Option<Vec<GenericParam>>,
    ) -> Node<TopLevelItem> {
        let type_copy = TypeName {
            symbol: type_name.symbol,
            span: type_name.span,
            id: self.alloc_id(),
        };
        let trait_ty = self.trait_type(trait_name);
        self.node(TopLevelItem {
            attrs: Vec::new(),
            pub_: false,
            decl: TopLevelDecl::Impl {
                type_name: type_copy,
                generics,
                unsafe_: false,
                trait_: Some(trait_ty),
                members,
            },
        })
    }

    fn trait_method(
        &mut self,
        name: &str,
        params: Vec<Param>,
        ret: Node<Type>,
        body: BlockNode,
    ) -> Function {
        Function {
            attrs: Vec::new(),
            derives: Vec::new(),
            directives: Vec::new(),
            unsafe_: false,
            name: self.ident(name),
            generics: None,
            params,
            ret: Some(ret),
            body,
        }
    }

    fn expr_block(&mut self, tail: ExprNode) -> BlockNode {
        self.node(Block {
            items: vec![BlockItem::Expr(tail)],
        })
    }

    fn enum_rhs_match_arm(
        &mut self,
        type_name: &TypeName,
        variant: &EnumVariant,
        idx: usize,
        lhs_bindings: &[Ident],
        rhs: &Ident,
        generic_params: Option<&[GenericParam]>,
    ) -> ExprNode {
        let (rhs_pat, rhs_bindings) = self.variant_pattern(type_name, variant, "b", idx);
        let then_body =
            self.variant_payload_eq(variant, lhs_bindings, &rhs_bindings, generic_params);
        let else_tail = self.bool_lit(false);
        let rhs_scrutinee = self.ident_expr(rhs);
        let wildcard = self.wildcard_pattern();
        self.match_expr(
            rhs_scrutinee,
            vec![
                MatchArm {
                    pattern: rhs_pat,
                    guard: None,
                    body: then_body,
                },
                MatchArm {
                    pattern: wildcard,
                    guard: None,
                    body: else_tail,
                },
            ],
        )
    }

    fn wildcard_pattern(&mut self) -> PatternNode {
        self.node(Pattern::Wildcard)
    }

    fn struct_partialeq_body(&mut self, type_name: &TypeName, body: &StructBody) -> BlockNode {
        let _ = type_name;
        let expr = match body {
            StructBody::Fields(fields) => {
                let comps: Vec<ExprNode> = fields
                    .iter()
                    .map(|f| self.field_eq_typed(&f.name, &f.name, &f.ty.inner))
                    .collect();
                self.and_chain_or_true(comps)
            }
            StructBody::Tuple(types) => {
                let comps: Vec<ExprNode> = (0..types.len())
                    .map(|i| {
                        let field = self.ident(&i.to_string());
                        self.field_eq_typed(&field, &field, &types[i].inner)
                    })
                    .collect();
                self.and_chain_or_true(comps)
            }
            StructBody::Unit => self.bool_lit(true),
        };
        self.expr_block(expr)
    }

    fn enum_partialeq_body(
        &mut self,
        type_name: &TypeName,
        variants: &[EnumVariant],
        generic_params: Option<&[GenericParam]>,
    ) -> BlockNode {
        let lhs = self.ident("__derived_lhs");
        let rhs = self.ident("__derived_rhs");
        let ty = self.type_named(type_name, generic_params);
        let mut lhs_arms = Vec::with_capacity(variants.len());
        for (i, v) in variants.iter().enumerate() {
            let (lhs_pat, lhs_bindings) = self.variant_pattern(type_name, v, "a", i);
            let body =
                self.enum_rhs_match_arm(type_name, v, i, &lhs_bindings, &rhs, generic_params);
            lhs_arms.push(MatchArm {
                pattern: lhs_pat,
                guard: None,
                body,
            });
        }
        let lhs_scrutinee = self.ident_expr(&lhs);
        let match_tail = self.match_expr(lhs_scrutinee, lhs_arms);
        let self_base = self.self_expr();
        let lhs_init = self.deref_expr(self_base);
        let other_base = self.other_expr();
        let rhs_init = self.deref_expr(other_base);
        let lhs_stmt = self.node(Stmt::Const {
            name: lhs,
            ty: Some(ty.clone()),
            init: lhs_init,
        });
        let rhs_stmt = self.node(Stmt::Const {
            name: rhs,
            ty: Some(ty),
            init: rhs_init,
        });
        self.node(Block {
            items: vec![
                BlockItem::Stmt(lhs_stmt),
                BlockItem::Stmt(rhs_stmt),
                BlockItem::Expr(match_tail),
            ],
        })
    }

    fn type_named(
        &mut self,
        type_name: &TypeName,
        generic_params: Option<&[GenericParam]>,
    ) -> Node<Type> {
        let name_id = self.alloc_id();
        let generic_args = generic_params
            .filter(|params| !params.is_empty())
            .map(|params| self.generic_type_args(params));
        self.node(Type::Named {
            name: TypeName {
                symbol: type_name.symbol,
                span: type_name.span,
                id: name_id,
            },
            generics: generic_args,
        })
    }

    fn variant_pattern(
        &mut self,
        _type_name: &TypeName,
        variant: &EnumVariant,
        prefix: &str,
        idx: usize,
    ) -> (PatternNode, Vec<Ident>) {
        let name = TypeName {
            symbol: variant.name.symbol,
            span: variant.name.span,
            id: self.alloc_id(),
        };
        match &variant.kind {
            Variant::Unit => {
                let id = self.alloc_id();
                let pat = self.node(Pattern::Ident(Ident {
                    symbol: variant.name.symbol,
                    span: variant.name.span,
                    id,
                }));
                (pat, Vec::new())
            }
            Variant::Tuple(types) => {
                let mut bindings = Vec::with_capacity(types.len());
                for f in 0..types.len() {
                    bindings.push(self.ident(&format!("__d_{prefix}_{idx}_{f}")));
                }
                let mut pats = Vec::with_capacity(bindings.len());
                for b in &bindings {
                    pats.push(self.node(Pattern::Ident(*b)));
                }
                let pat = self.node(Pattern::Tuple {
                    name,
                    patterns: pats,
                });
                (pat, bindings)
            }
            Variant::Struct(fields) => {
                let bindings: Vec<Ident> = fields
                    .iter()
                    .enumerate()
                    .map(|(f, _)| self.ident(&format!("__d_{prefix}_{idx}_{f}")))
                    .collect();
                let mut pat_fields = Vec::with_capacity(fields.len());
                for (field, bind) in fields.iter().zip(bindings.iter()) {
                    let field_id = self.alloc_id();
                    let bind_pat = self.node(Pattern::Ident(*bind));
                    pat_fields.push(StructPatternField {
                        name: Ident {
                            symbol: field.name.symbol,
                            span: field.name.span,
                            id: field_id,
                        },
                        pattern: Some(Box::new(bind_pat)),
                    });
                }
                let pat = self.node(Pattern::Struct {
                    name,
                    fields: pat_fields,
                });
                (pat, bindings)
            }
        }
    }

    fn variant_payload_eq(
        &mut self,
        variant: &EnumVariant,
        self_bindings: &[Ident],
        other_bindings: &[Ident],
        generic_params: Option<&[GenericParam]>,
    ) -> ExprNode {
        match &variant.kind {
            Variant::Tuple(types) => {
                let comps: Vec<ExprNode> = self_bindings
                    .iter()
                    .zip(other_bindings.iter())
                    .zip(types.iter())
                    .map(|((a, b), ty)| self.ident_eq_typed(a, b, &ty.inner, generic_params))
                    .collect();
                self.and_chain_or_true(comps)
            }
            Variant::Struct(fields) => {
                let comps: Vec<ExprNode> = self_bindings
                    .iter()
                    .zip(other_bindings.iter())
                    .zip(fields.iter())
                    .map(|((a, b), field)| {
                        self.ident_eq_typed(a, b, &field.ty.inner, generic_params)
                    })
                    .collect();
                self.and_chain_or_true(comps)
            }
            Variant::Unit => self.bool_lit(true),
        }
    }

    fn and_chain_or_true(&mut self, exprs: Vec<ExprNode>) -> ExprNode {
        match self.and_chain(exprs) {
            Some(expr) => expr,
            None => self.bool_lit(true),
        }
    }

    fn field_eq_typed(&mut self, field: &Ident, other_field: &Ident, ty: &Type) -> ExprNode {
        let self_base = self.self_expr();
        let other_base = self.other_expr();
        let lhs = self.field_access(self_base, field);
        let rhs = self.field_access(other_base, other_field);
        if self.is_global_type(ty) {
            return self.bool_lit(true);
        }
        self.bin_eq(lhs, rhs)
    }

    fn is_global_type(&self, ty: &Type) -> bool {
        matches!(
            ty,
            Type::Named { name, generics: None }
                if self.interner.resolves_to(name.symbol, "Global")
        )
    }

    fn ident_eq_typed(
        &mut self,
        a: &Ident,
        b: &Ident,
        _ty: &Type,
        _generic_params: Option<&[GenericParam]>,
    ) -> ExprNode {
        let lhs = self.ident_expr(a);
        let rhs = self.ident_expr(b);
        self.bin_eq(lhs, rhs)
    }

    fn and_chain(&mut self, mut exprs: Vec<ExprNode>) -> Option<ExprNode> {
        if exprs.is_empty() {
            return None;
        }
        while exprs.len() > 1 {
            let rhs = exprs.pop()?;
            let lhs = exprs.pop()?;
            let merged = self.if_bool_and(lhs, rhs);
            exprs.push(merged);
        }
        exprs.pop()
    }

    fn if_bool_and(&mut self, lhs: ExprNode, rhs: ExprNode) -> ExprNode {
        let then_block = self.expr_block(rhs);
        let false_lit = self.bool_lit(false);
        let else_block = self.expr_block(false_lit);
        self.node(Expr::If {
            condition: Box::new(IfCondition::Bool(lhs)),
            then_block,
            else_ifs: Vec::new(),
            else_block: Some(else_block),
        })
    }

    fn match_expr(&mut self, scrutinee: ExprNode, arms: Vec<MatchArm>) -> ExprNode {
        self.node(Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
        })
    }

    fn deref_expr(&mut self, base: ExprNode) -> ExprNode {
        self.node(Expr::Unary {
            op: UnaryOp::Deref,
            operand: Box::new(base),
        })
    }

    fn self_expr(&mut self) -> ExprNode {
        let id = self.alloc_id();
        self.node(Expr::Ident(Ident {
            symbol: impl_receiver_symbol(),
            span: self.span,
            id,
        }))
    }

    fn other_expr(&mut self) -> ExprNode {
        let other = self.ident("other");
        self.ident_expr(&other)
    }

    fn ident_expr(&mut self, id: &Ident) -> ExprNode {
        self.node(Expr::Ident(*id))
    }

    fn field_access(&mut self, base: ExprNode, field: &Ident) -> ExprNode {
        let field_id = self.alloc_id();
        self.node(Expr::Postfix {
            base: Box::new(base),
            ops: vec![PostfixOp::Field(Ident {
                symbol: field.symbol,
                span: field.span,
                id: field_id,
            })],
        })
    }

    fn bin_eq(&mut self, left: ExprNode, right: ExprNode) -> ExprNode {
        self.node(Expr::Binary {
            op: BinOp::Eq,
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    fn bool_lit(&mut self, value: bool) -> ExprNode {
        self.node(Expr::Literal(Literal::Bool(value)))
    }

    fn u8_array_literal(&mut self, bytes: &[u8; 32]) -> ExprNode {
        let items: Vec<ExprNode> = bytes
            .iter()
            .map(|&b| self.node(Expr::Literal(Literal::ByteChar(b))))
            .collect();
        self.node(Expr::Array(items))
    }
}

/// Left-pads a type name into a fixed 32-byte buffer for MVP `Debug::fmt` output.
///
/// Names longer than 32 bytes are truncated; shorter names are zero-filled.
fn debug_name_bytes(name: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, b) in name.bytes().enumerate().take(32) {
        out[i] = b;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_surfaces_intern_table_full_error() {
        let mut interner = Interner::new();
        let span = Span::new(0, 4);
        let mut ast_gen = AstGen::new(&mut interner, span);
        ast_gen.failed = Some(DeriveError {
            span,
            message: "identifier intern table is full during derive expansion".to_string(),
        });
        let type_name = TypeName {
            symbol: Symbol::from_raw(0),
            span,
            id: ast_gen.alloc_id(),
        };
        let item = ast_gen.node(TopLevelItem {
            attrs: Vec::new(),
            pub_: false,
            decl: TopLevelDecl::Struct {
                name: type_name,
                derives: Vec::new(),
                generics: None,
                body: StructBody::Unit,
            },
        });
        let Err(err) = ast_gen.finish(item) else {
            panic!("expected derive error");
        };
        assert!(err.message.contains("intern table is full"));
    }

    #[test]
    fn intern_sym_poison_after_failure_does_not_overwrite_error() {
        let mut interner = Interner::new();
        let span = Span::new(0, 1);
        let mut ast_gen = AstGen::new(&mut interner, span);
        ast_gen.failed = Some(DeriveError {
            span,
            message: "identifier intern table is full during derive expansion".to_string(),
        });
        let sym = ast_gen.intern_sym("unused");
        assert_eq!(sym, Symbol::from_raw(0));
        assert!(
            ast_gen
                .failed
                .as_ref()
                .is_some_and(|e| e.message.contains("intern table is full"))
        );
    }
}

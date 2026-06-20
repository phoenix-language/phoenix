//! Match arms, patterns, and exhaustiveness.
//!
//! Type-checks `match` expressions: binds pattern variables, unifies arm body types, and validates
//! reachability and exhaustiveness against the scrutinee type. Invoked from [`super::expr`] when
//! checking [`Expr::Match`](phx_syntax::ast::expr::Expr::Match).
//!
//! # Responsibilities
//!
//! - **Pattern binding** — [`TypeChecker::check_pattern`] introduces locals (or match temps) with
//!   types derived from struct fields, enum variant payloads, or the scrutinee type for wildcards
//!   and identifiers.
//! - **Arm typing** — each arm body is checked under an ownership fork; results join via
//!   [`crate::typeck::unify::unify_branch`].
//! - **Diagnostics** — unreachable arms (duplicate literals, shadowed `_`) and non-exhaustive
//!   matches on enums and `bool` are reported without stopping the walk.
//!
//! # Key entry points
//!
//! | Function | Role |
//! |----------|------|
//! | [`TypeChecker::check_match`] | Check scrutinee, all arms, join ownership, return arm type. |
//! | [`TypeChecker::check_pattern`] | Recursively bind/destructure one [`Pattern`]. |
//! | [`TypeChecker::check_match_exhaustiveness`] | Ensure enum/bool scrutinees are fully covered. |
//! | [`TypeChecker::check_match_unreachable_arms`] | Flag arms shadowed by earlier patterns. |

use std::collections::HashMap;

use phx_diagnostics::{MismatchKind, Span, TypeCheckError};
use phx_syntax::Symbol;
use phx_syntax::ast::ExprNode;
use phx_syntax::ast::expr::StructFieldInit;
use phx_syntax::ast::ident::TypeName;
use phx_syntax::ast::lit::Literal;
use phx_syntax::ast::pat::Pattern;

use super::TypeChecker;
use crate::resolver::{DefId, DefKind};
use crate::typeck::bindings::BindingKind;
use crate::typeck::layout::VariantKind;
use crate::typeck::mono::{TypeMonoKind, generic_param_defs_for_type, generic_params_for_def};
use crate::typeck::ownership::OwnershipTracker;
use crate::typeck::primitive::is_int_keyword;
use crate::typeck::types::{Ty, TypeId};
use crate::typeck::unify::unify_branch;
use phx_syntax::token::Keyword;

impl TypeChecker<'_> {
    /// Type-checks a `match`: scrutinee expression, arms, guards, and unified result type.
    ///
    /// Forks ownership per arm and joins with [`OwnershipTracker::join_arms`](crate::typeck::ownership::OwnershipTracker::join_arms).
    /// Allocates a match scrutinee temp when layout emission is active. Returns [`TypeChecker::unit`]
    /// when there are no arms or branch types fail to unify.
    pub(in crate::typeck::check) fn check_match(
        &mut self,
        scrutinee: &ExprNode,
        arms: &[phx_syntax::ast::pat::MatchArm],
        span: Span,
    ) -> TypeId {
        let s = self.check_expr_node(scrutinee);
        self.record_scrutinee_type_mono(s, span);
        if let Some(layout) = &mut self.layout {
            let _ = layout.alloc_match_scrutinee_temp(s, span);
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

    /// Reports match arms that can never execute because an earlier arm already covers them.
    pub(in crate::typeck::check) fn check_match_unreachable_arms(
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

    /// Ensures `match` on enums and `bool` covers every variant or literal value.
    ///
    /// Wildcard arms satisfy exhaustiveness for any scrutinee type. Enum scrutinees require every
    /// variant name to appear in at least one arm pattern.
    pub(in crate::typeck::check) fn check_match_exhaustiveness(
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

    pub(in crate::typeck::check) fn pattern_covered_variant(
        &self,
        pat: &Pattern,
    ) -> Option<Symbol> {
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

    pub(in crate::typeck::check) fn scrutinee_enum_def(&self, scrutinee: TypeId) -> Option<DefId> {
        match self.types.get(scrutinee) {
            Ty::Named { def, .. } if self.program_layout.enums.contains_key(def) => Some(*def),
            _ => None,
        }
    }

    pub(in crate::typeck::check) fn named_type_args(
        &self,
        ty: TypeId,
    ) -> Option<(DefId, Vec<TypeId>)> {
        match self.types.get(ty) {
            Ty::Named { def, args } => Some((*def, args.clone())),
            _ => None,
        }
    }

    pub(in crate::typeck::check) fn variant_payload_for_scrutinee(
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

    pub(in crate::typeck::check) fn error_enum_pattern_on_non_enum(
        &mut self,
        scrutinee: TypeId,
        span: Span,
    ) {
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

    pub(in crate::typeck::check) fn error_enum_variant_mismatch(
        &mut self,
        expected_def: DefId,
        scrutinee: TypeId,
        span: Span,
    ) {
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

    /// Recursively checks `pat` against `scrutinee`, introducing bindings with `binding_kind`.
    ///
    /// Struct and enum patterns destructure fields or tuple payloads from
    /// [`ProgramLayout`](crate::typeck::layout::ProgramLayout). Bare identifiers that name enum
    /// variants are validated against the scrutinee enum; other identifiers become locals.
    #[allow(clippy::too_many_lines)]
    pub(in crate::typeck::check) fn check_pattern(
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
                    self.define_local(ident.symbol, scrutinee, binding_kind, None, ident.span);
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
                                self.define_local(
                                    field.name.symbol,
                                    fty,
                                    binding_kind,
                                    None,
                                    field.name.span,
                                );
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
                                    self.define_local(
                                        field.name.symbol,
                                        *fty,
                                        binding_kind,
                                        None,
                                        field.name.span,
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

    pub(in crate::typeck::check) fn format_named(&self, def: DefId) -> String {
        self.resolved
            .defs
            .get(def.index() as usize)
            .map(|d| format!("type#{}", d.name.index()))
            .unwrap_or_else(|| "<?>".to_string())
    }

    #[allow(clippy::too_many_lines)]
    pub(in crate::typeck::check) fn check_struct_lit(
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

    pub(in crate::typeck::check) fn struct_lit_type_args(
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

    pub(in crate::typeck::check) fn check_struct_type_lit(
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

    pub(in crate::typeck::check) fn check_enum_struct_variant_lit(
        &mut self,
        enum_def: DefId,
        variant: crate::typeck::layout::VariantLayout,
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
            self.record_type_mono_inst(enum_def, TypeMonoKind::Enum, type_args.clone(), span);
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

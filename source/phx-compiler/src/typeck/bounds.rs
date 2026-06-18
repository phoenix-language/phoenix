//! Trait bound checking at generic instantiation sites.

use std::collections::HashMap;

use phx_diagnostics::{TypeCheckBag, TypeCheckError};
use phx_syntax::Span;
use phx_syntax::ast::Node;
use phx_syntax::ast::Type;
use phx_syntax::ast::types::GenericParam;

use super::builtins::is_copyable;
use super::layout::{ProgramLayout, TraitImplementer, TraitInstKey};
use super::lower_ty::{build_type_def_map, lower_type};
use super::subst::Substitution;
use super::types::{Ty, TypeId, TypeInterner};
use super::unify::AliasEnv;
use crate::lang_items::LangItemRegistry;
use crate::resolver::{DefId, DefKind, ResolvedProgram};

/// Validates that each concrete type argument satisfies the corresponding generic parameter bounds.
///
/// Returns `false` when any bound fails (errors are appended to `bag`).
#[must_use]
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn validate_instantiation_bounds(
    resolved: &ResolvedProgram,
    layout: &ProgramLayout,
    types: &mut TypeInterner,
    std_traits: &LangItemRegistry,
    value_types: &HashMap<DefId, TypeId>,
    generics: Option<&[GenericParam]>,
    param_defs: &[DefId],
    concrete_args: &[TypeId],
    module: u32,
    span: Span,
    bag: &mut TypeCheckBag,
) -> bool {
    let Some(generic_params) = generics else {
        return true;
    };
    if generic_params.len() != param_defs.len() || param_defs.len() != concrete_args.len() {
        bag.push(
            module,
            TypeCheckError::ArityMismatch {
                expected: param_defs.len(),
                found: concrete_args.len(),
                span,
            },
        );
        return false;
    }
    let mut subst = Substitution::new();
    for (param_def, concrete) in param_defs.iter().zip(concrete_args) {
        subst.insert(*param_def, *concrete);
    }
    let type_defs = build_type_def_map(&resolved.defs);
    let mut ok = true;
    for ((param, &param_def), &concrete) in generic_params.iter().zip(param_defs).zip(concrete_args)
    {
        if should_skip_unresolved_generic_bound(resolved, types, param_def, concrete) {
            continue;
        }
        let Some(bounds) = param.bounds.as_ref() else {
            continue;
        };
        for bound in bounds {
            let Some((trait_symbol, trait_arg_nodes)) = trait_bound_head(&bound.inner) else {
                bag.push(
                    module,
                    TypeCheckError::UnsupportedFeature {
                        feature: "non-trait generic bound",
                        span: bound.span,
                    },
                );
                ok = false;
                continue;
            };
            let is_copyable_bound = trait_arg_nodes.is_none()
                && (resolved.interner.resolves_to(trait_symbol, "Copyable")
                    || std_traits
                        .trait_def_for_symbol(&resolved.interner, trait_symbol)
                        .is_some_and(|def| std_traits.is_copyable_trait(def)));
            if is_copyable_bound {
                if !type_satisfies_copyable(types, layout, std_traits, concrete) {
                    let type_name = format_type_name(resolved, types, concrete);
                    bag.push(
                        module,
                        TypeCheckError::TraitNotSatisfied {
                            type_name,
                            trait_name: "Copyable".to_owned(),
                            span,
                        },
                    );
                    ok = false;
                }
                continue;
            }
            let Some(trait_def) = resolve_trait_def(resolved, std_traits, trait_symbol) else {
                bag.push(
                    module,
                    TypeCheckError::UnknownTraitBound {
                        trait_name: format_trait_bound(resolved, types, &bound.inner),
                        span: bound.span,
                    },
                );
                ok = false;
                continue;
            };
            let trait_args =
                lower_trait_bound_args(types, &type_defs, trait_arg_nodes, &subst, resolved);
            let alias_env = AliasEnv {
                types,
                defs: &resolved.defs,
                value_types,
            };
            if !type_satisfies_trait_inst(
                layout,
                types,
                std_traits,
                concrete,
                trait_def,
                &trait_args,
                Some(&alias_env),
            ) {
                let type_name = format_type_name(resolved, types, concrete);
                let trait_name = format_trait_bound(resolved, types, &bound.inner);
                bag.push(
                    module,
                    TypeCheckError::TraitNotSatisfied {
                        type_name,
                        trait_name,
                        span,
                    },
                );
                ok = false;
            }
        }
        let _ = param_def;
    }
    ok
}

/// Returns the trait name and optional generic arguments from a bound type.
#[must_use]
pub fn trait_bound_head(ty: &Type) -> Option<(phx_syntax::Symbol, Option<&[Node<Type>]>)> {
    match ty {
        Type::Named { name, generics } => Some((name.symbol, generics.as_deref())),
        _ => None,
    }
}

fn lower_trait_bound_args(
    types: &mut TypeInterner,
    type_defs: &super::lower_ty::TypeDefMap,
    trait_arg_nodes: Option<&[Node<Type>]>,
    subst: &Substitution,
    resolved: &ResolvedProgram,
) -> Vec<TypeId> {
    let Some(nodes) = trait_arg_nodes else {
        return Vec::new();
    };
    nodes
        .iter()
        .map(|node| {
            let id = lower_type(types, type_defs, &node.inner);
            Substitution::apply(types, id, subst, resolved)
        })
        .collect()
}

fn resolve_trait_def(
    resolved: &ResolvedProgram,
    std_traits: &LangItemRegistry,
    trait_symbol: phx_syntax::Symbol,
) -> Option<DefId> {
    if let Some(def) = std_traits.trait_def_for_symbol(&resolved.interner, trait_symbol) {
        return Some(def);
    }
    resolved.defs.iter().enumerate().find_map(|(i, d)| {
        if d.kind == DefKind::Trait && d.name == trait_symbol {
            Some(DefId::from_raw(u32::try_from(i).ok()?))
        } else {
            None
        }
    })
}

fn should_skip_unresolved_generic_bound(
    resolved: &ResolvedProgram,
    types: &TypeInterner,
    param_def: DefId,
    concrete: TypeId,
) -> bool {
    let Ty::Named {
        def: concrete_def,
        args,
    } = types.get(concrete)
    else {
        return false;
    };
    if !args.is_empty() {
        return false;
    }
    if *concrete_def == param_def {
        return true;
    }
    let Some(param_record) = resolved.defs.get(param_def.index() as usize) else {
        return false;
    };
    let Some(concrete_record) = resolved.defs.get(concrete_def.index() as usize) else {
        return false;
    };
    param_record.kind == DefKind::GenericParam
        && concrete_record.kind == DefKind::GenericParam
        && param_record.name == concrete_record.name
}

fn type_satisfies_copyable(
    types: &TypeInterner,
    layout: &ProgramLayout,
    std_traits: &LangItemRegistry,
    concrete: TypeId,
) -> bool {
    is_copyable(types, layout, std_traits, concrete)
}

/// Returns `true` when `concrete` implements `trait_def` with the given trait type arguments.
#[must_use]
pub fn type_satisfies_trait_inst(
    layout: &ProgramLayout,
    types: &TypeInterner,
    std_traits: &LangItemRegistry,
    concrete: TypeId,
    trait_def: DefId,
    trait_args: &[TypeId],
    aliases: Option<&AliasEnv<'_>>,
) -> bool {
    let concrete = aliases
        .map(|env| super::unify::normalize_type(env, concrete))
        .unwrap_or(concrete);
    if std_traits.is_copyable_trait(trait_def) && trait_args.is_empty() {
        return type_satisfies_copyable(types, layout, std_traits, concrete);
    }
    match types.get(concrete) {
        Ty::Primitive(kw) => {
            trait_args.is_empty()
                && (std_traits.primitive_satisfies(*kw, trait_def)
                    || layout_has_builtin_trait_impl(layout, *kw, trait_def, trait_args))
        }
        Ty::Str => {
            trait_args.is_empty() && layout_has_str_trait_impl(layout, trait_def, trait_args)
        }
        Ty::Unit => {
            trait_args.is_empty()
                && (std_traits.copyable_trait == Some(trait_def)
                    || std_traits.clone_trait == Some(trait_def)
                    || std_traits.partial_eq_trait == Some(trait_def)
                    || std_traits.eq_trait == Some(trait_def)
                    || std_traits.debug_trait == Some(trait_def))
        }
        Ty::Named { def, args } => layout_has_trait_impl(layout, *def, args, trait_def, trait_args),
        Ty::Tuple(elems) => {
            trait_args.is_empty()
                && elems.iter().all(|e| {
                    type_satisfies_trait_inst(
                        layout, types, std_traits, *e, trait_def, trait_args, aliases,
                    )
                })
        }
        Ty::Array { elem, .. } => {
            trait_args.is_empty()
                && type_satisfies_trait_inst(
                    layout, types, std_traits, *elem, trait_def, trait_args, aliases,
                )
        }
        _ => false,
    }
}

fn layout_has_trait_impl(
    layout: &ProgramLayout,
    implementer: DefId,
    implementer_args: &[TypeId],
    trait_def: DefId,
    trait_args: &[TypeId],
) -> bool {
    let key = TraitInstKey::new(
        TraitImplementer::Type(implementer),
        implementer_args.to_vec(),
        trait_def,
        trait_args.to_vec(),
    );
    if layout.trait_impls.contains(&key) {
        return true;
    }
    layout.trait_methods.keys().any(|(inst, _)| {
        inst.implementer == TraitImplementer::Type(implementer)
            && inst.implementer_args == implementer_args
            && inst.trait_def == trait_def
            && inst.trait_args == trait_args
    })
}

fn layout_has_builtin_trait_impl(
    layout: &ProgramLayout,
    kw: phx_syntax::token::Keyword,
    trait_def: DefId,
    trait_args: &[TypeId],
) -> bool {
    let key = TraitInstKey::new(
        TraitImplementer::Primitive(kw),
        Vec::new(),
        trait_def,
        trait_args.to_vec(),
    );
    layout.trait_impls.contains(&key)
        || layout.trait_methods.keys().any(|(inst, _)| {
            inst.implementer == TraitImplementer::Primitive(kw)
                && inst.trait_def == trait_def
                && inst.trait_args == trait_args
        })
}

fn layout_has_str_trait_impl(
    layout: &ProgramLayout,
    trait_def: DefId,
    trait_args: &[TypeId],
) -> bool {
    let key = TraitInstKey::new(
        TraitImplementer::Str,
        Vec::new(),
        trait_def,
        trait_args.to_vec(),
    );
    layout.trait_impls.contains(&key)
        || layout.trait_methods.keys().any(|(inst, _)| {
            inst.implementer == TraitImplementer::Str
                && inst.trait_def == trait_def
                && inst.trait_args == trait_args
        })
}

/// Resolves `From::from` for `E_out: From<E_in>` when the trait impl exists.
#[must_use]
pub fn resolve_from_fn_for_error(
    layout: &ProgramLayout,
    types: &TypeInterner,
    std_traits: &LangItemRegistry,
    resolved: &ResolvedProgram,
    err_out: TypeId,
    err_in: TypeId,
) -> Option<DefId> {
    let from_trait_def = std_traits.from_trait?;
    let Ty::Named {
        def: implementer,
        args: implementer_args,
    } = types.get(err_out).clone()
    else {
        return None;
    };
    if !type_satisfies_trait_inst(
        layout,
        types,
        std_traits,
        err_out,
        from_trait_def,
        &[err_in],
        None,
    ) {
        return None;
    }
    let key = TraitInstKey::new(
        TraitImplementer::Type(implementer),
        implementer_args.clone(),
        from_trait_def,
        vec![err_in],
    );
    layout
        .trait_methods
        .iter()
        .find(|((k, method), _)| k == &key && resolved.interner.resolves_to(*method, "from"))
        .map(|(_, fn_def)| *fn_def)
}

fn format_type_name(resolved: &ResolvedProgram, types: &TypeInterner, id: TypeId) -> String {
    super::display::format_type(types, &resolved.interner, &resolved.defs, id)
}

fn format_trait_bound(resolved: &ResolvedProgram, types: &TypeInterner, ty: &Type) -> String {
    match ty {
        Type::Named {
            name,
            generics: None,
        } => resolved.interner.resolve_display(name.symbol),
        Type::Named {
            name,
            generics: Some(args),
        } => {
            let head = resolved.interner.resolve(name.symbol).unwrap_or("<?>");
            let type_defs = build_type_def_map(&resolved.defs);
            let mut interner = types.clone();
            let arg_strs: Vec<String> = args
                .iter()
                .map(|a| {
                    let id = lower_type(&mut interner, &type_defs, &a.inner);
                    format_type_name(resolved, &interner, id)
                })
                .collect();
            format!("{head}<{}>", arg_strs.join(", "))
        }
        _ => "?".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use phx_syntax::Interner;
    use phx_syntax::ast::Program;
    use phx_syntax::ast::ident::Ident;
    use phx_syntax::ast::node_id::AstNodeId;
    use phx_syntax::intern::Symbol;

    use super::*;
    use crate::attrs::DefAttrs;

    #[test]
    fn validate_instantiation_bounds_arity_mismatch_errors() {
        let resolved = ResolvedProgram {
            program: Program {
                imports: Vec::new(),
                items: Vec::new(),
            },
            modules: Vec::new(),
            root: 0,
            interner: Interner::new(),
            defs: Vec::new(),
            resolutions: HashMap::new(),
            closures: HashMap::new(),
            main_fn: None,
            import_types: HashMap::new(),
            import_lang_items: HashMap::new(),
            def_attrs: DefAttrs::new(),
        };
        let layout = ProgramLayout::default();
        let mut types = TypeInterner::default();
        let std_traits = LangItemRegistry::default();
        let value_types = HashMap::new();
        let span = Span::new(0, 0);
        let mut bag = TypeCheckBag::new();
        let generic_param = GenericParam {
            name: Ident {
                symbol: Symbol::from_raw(1),
                span,
                id: AstNodeId::synthetic(0),
            },
            bounds: None,
            default: None,
        };
        let generics = [generic_param];

        let ok = validate_instantiation_bounds(
            &resolved,
            &layout,
            &mut types,
            &std_traits,
            &value_types,
            Some(&generics),
            &[],
            &[],
            0,
            span,
            &mut bag,
        );

        assert!(!ok);
        assert!(
            bag.errors().iter().any(|e| {
                matches!(
                    &e.error,
                    TypeCheckError::ArityMismatch {
                        expected: 0,
                        found: 0,
                        ..
                    }
                )
            }),
            "expected ArityMismatch for generic metadata length mismatch: {:?}",
            bag.errors()
        );
    }
}

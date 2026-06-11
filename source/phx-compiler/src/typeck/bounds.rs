//! Trait bound checking at generic instantiation sites.

use std::collections::HashMap;

use phx_diagnostics::{TypeCheckBag, TypeCheckError};
use phx_syntax::Span;
use phx_syntax::ast::Node;
use phx_syntax::ast::Type;
use phx_syntax::ast::types::GenericParam;

use super::builtins::is_copyable;
use super::layout::{ProgramLayout, TraitInstKey};
use super::lower_ty::{build_type_def_map, lower_type};
use super::std_trait_kernel::StdTraitKernel;
use super::subst::Substitution;
use super::types::{Ty, TypeId, TypeInterner};
use super::unify::AliasEnv;
use crate::resolver::{DefId, DefKind, ResolvedProgram};

/// Validates that each concrete type argument satisfies the corresponding generic parameter bounds.
///
/// Returns `false` when any bound fails (errors are appended to `bag`).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn validate_instantiation_bounds(
    resolved: &ResolvedProgram,
    layout: &ProgramLayout,
    types: &TypeInterner,
    std_traits: &StdTraitKernel,
    value_types: &HashMap<DefId, TypeId>,
    generics: Option<&[GenericParam]>,
    param_defs: &[DefId],
    concrete_args: &[TypeId],
    module: u32,
    span: Span,
    bag: &mut TypeCheckBag,
) -> bool {
    let alias_env = AliasEnv {
        types,
        defs: &resolved.defs,
        value_types,
    };
    let Some(generic_params) = generics else {
        return true;
    };
    if generic_params.len() != param_defs.len() || param_defs.len() != concrete_args.len() {
        return true;
    }
    let mut subst = Substitution::new();
    for (param_def, concrete) in param_defs.iter().zip(concrete_args) {
        subst.insert(*param_def, *concrete);
    }
    let type_defs = build_type_def_map(&resolved.defs);
    let mut ok = true;
    for ((param, &param_def), &concrete) in generic_params.iter().zip(param_defs).zip(concrete_args)
    {
        if let Ty::Named {
            def: concrete_def,
            args,
        } = types.get(concrete)
            && *concrete_def == param_def
            && args.is_empty()
        {
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
            let bound_name = resolved.interner.resolve(trait_symbol);
            if is_copyable_bound_name(bound_name, std_traits, trait_symbol, resolved)
                && trait_arg_nodes.is_none()
            {
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
            let trait_args = lower_trait_bound_args(types, &type_defs, trait_arg_nodes, &subst);
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
    types: &TypeInterner,
    type_defs: &super::lower_ty::TypeDefMap,
    trait_arg_nodes: Option<&[Node<Type>]>,
    subst: &Substitution,
) -> Vec<TypeId> {
    let Some(nodes) = trait_arg_nodes else {
        return Vec::new();
    };
    let mut interner = types.clone();
    nodes
        .iter()
        .map(|node| {
            let id = lower_type(&mut interner, type_defs, &node.inner);
            Substitution::apply(&mut interner, id, subst)
        })
        .collect()
}

fn is_copyable_bound_name(
    name: &str,
    std_traits: &StdTraitKernel,
    symbol: phx_syntax::Symbol,
    resolved: &ResolvedProgram,
) -> bool {
    if name == "Copyable" {
        return true;
    }
    if let Some(trait_def) = std_traits.trait_def_for_name(&resolved.interner, "Copyable") {
        if resolve_trait_def(resolved, std_traits, symbol) == Some(trait_def) {
            return true;
        }
    }
    false
}

fn resolve_trait_def(
    resolved: &ResolvedProgram,
    std_traits: &StdTraitKernel,
    trait_symbol: phx_syntax::Symbol,
) -> Option<DefId> {
    let name = resolved.interner.resolve(trait_symbol);
    if let Some(def) = std_traits.trait_def_for_name(&resolved.interner, name) {
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

fn type_satisfies_copyable(
    types: &TypeInterner,
    layout: &ProgramLayout,
    std_traits: &StdTraitKernel,
    concrete: TypeId,
) -> bool {
    is_copyable(types, layout, std_traits, concrete)
}

/// Returns `true` when `concrete` implements `trait_def` with the given trait type arguments.
#[must_use]
pub fn type_satisfies_trait_inst(
    layout: &ProgramLayout,
    types: &TypeInterner,
    std_traits: &StdTraitKernel,
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
            trait_args.is_empty() && std_traits.primitive_satisfies(*kw, trait_def)
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
        implementer,
        implementer_args.to_vec(),
        trait_def,
        trait_args.to_vec(),
    );
    if layout.trait_impls.contains(&key) {
        return true;
    }
    layout.trait_methods.keys().any(|(inst, _)| {
        inst.implementer == implementer
            && inst.implementer_args == implementer_args
            && inst.trait_def == trait_def
            && inst.trait_args == trait_args
    })
}

/// Resolves `From::from` for `E_out: From<E_in>` when the trait impl exists.
#[must_use]
pub fn resolve_from_fn_for_error(
    layout: &ProgramLayout,
    types: &TypeInterner,
    std_traits: &StdTraitKernel,
    resolved: &ResolvedProgram,
    err_out: TypeId,
    err_in: TypeId,
) -> Option<DefId> {
    let from_trait_def = resolve_trait_def_by_name(resolved, std_traits, "From")?;
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
    let key = TraitInstKey::new(implementer, implementer_args, from_trait_def, vec![err_in]);
    layout
        .trait_methods
        .iter()
        .find(|((k, method), _)| k == &key && resolved.interner.resolve(*method) == "from")
        .map(|(_, fn_def)| *fn_def)
}

fn resolve_trait_def_by_name(
    resolved: &ResolvedProgram,
    std_traits: &StdTraitKernel,
    trait_name: &str,
) -> Option<DefId> {
    if let Some(def) = std_traits.trait_def_for_name(&resolved.interner, trait_name) {
        return Some(def);
    }
    resolved.defs.iter().enumerate().find_map(|(i, d)| {
        if d.kind == DefKind::Trait && resolved.interner.resolve(d.name) == trait_name {
            Some(DefId::from_raw(u32::try_from(i).ok()?))
        } else {
            None
        }
    })
}

fn format_type_name(resolved: &ResolvedProgram, types: &TypeInterner, id: TypeId) -> String {
    super::display::format_type(types, &resolved.interner, &resolved.defs, id)
}

fn format_trait_bound(resolved: &ResolvedProgram, types: &TypeInterner, ty: &Type) -> String {
    match ty {
        Type::Named {
            name,
            generics: None,
        } => resolved.interner.resolve(name.symbol).to_owned(),
        Type::Named {
            name,
            generics: Some(args),
        } => {
            let head = resolved.interner.resolve(name.symbol);
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

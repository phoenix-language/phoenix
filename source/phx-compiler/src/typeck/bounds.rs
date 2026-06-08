//! Trait bound checking at generic instantiation sites.

use phx_diagnostics::{TypeCheckBag, TypeCheckError};
use phx_syntax::Span;
use phx_syntax::ast::types::GenericParam;

use super::builtins::is_copyable;
use super::layout::ProgramLayout;
use super::std_trait_kernel::StdTraitKernel;
use super::types::{Ty, TypeId, TypeInterner};
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
        return true;
    }
    let mut ok = true;
    for ((param, &param_def), &concrete) in generic_params.iter().zip(param_defs).zip(concrete_args)
    {
        let Some(bounds) = param.bounds.as_ref() else {
            continue;
        };
        for bound in bounds {
            let bound_name = resolved.interner.resolve(bound.symbol);
            if is_copyable_bound_name(bound_name, std_traits, bound.symbol, resolved) {
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
            let Some(trait_def) = resolve_trait_def(resolved, std_traits, bound.symbol) else {
                bag.push(
                    module,
                    TypeCheckError::UnknownTraitBound {
                        trait_name: bound_name.to_owned(),
                        span,
                    },
                );
                ok = false;
                continue;
            };
            if !type_satisfies_trait(layout, types, std_traits, concrete, trait_def) {
                let type_name = format_type_name(resolved, types, concrete);
                let trait_name = resolved.interner.resolve(bound.symbol).to_owned();
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

fn type_satisfies_trait(
    layout: &ProgramLayout,
    types: &TypeInterner,
    std_traits: &StdTraitKernel,
    concrete: TypeId,
    trait_def: DefId,
) -> bool {
    if std_traits.is_copyable_trait(trait_def) {
        return type_satisfies_copyable(types, layout, std_traits, concrete);
    }
    match types.get(concrete) {
        Ty::Primitive(kw) => std_traits.primitive_satisfies(*kw, trait_def),
        Ty::Unit => {
            std_traits.copyable_trait == Some(trait_def)
                || std_traits.clone_trait == Some(trait_def)
                || std_traits.partial_eq_trait == Some(trait_def)
                || std_traits.eq_trait == Some(trait_def)
                || std_traits.debug_trait == Some(trait_def)
        }
        Ty::Named { def, .. } => {
            layout.trait_impls.contains(&(*def, trait_def))
                || layout
                    .trait_methods
                    .keys()
                    .any(|(type_def, bound_trait, _)| {
                        *type_def == *def && *bound_trait == trait_def
                    })
        }
        Ty::Tuple(elems) => elems
            .iter()
            .all(|e| type_satisfies_trait(layout, types, std_traits, *e, trait_def)),
        Ty::Array { elem, .. } => type_satisfies_trait(layout, types, std_traits, *elem, trait_def),
        _ => false,
    }
}

fn format_type_name(resolved: &ResolvedProgram, types: &TypeInterner, id: TypeId) -> String {
    super::display::format_type(types, &resolved.interner, &resolved.defs, id)
}

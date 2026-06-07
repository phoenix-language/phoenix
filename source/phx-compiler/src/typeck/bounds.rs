//! Trait bound checking at generic instantiation sites.

use phx_diagnostics::{TypeCheckBag, TypeCheckError};
use phx_syntax::Span;
use phx_syntax::ast::types::GenericParam;

use super::builtins::is_copyable;
use super::layout::ProgramLayout;
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
            if is_copyable_trait_name(resolved, bound.symbol) {
                if !is_copyable(types, concrete) {
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
            let Some(trait_def) = resolve_trait_def(resolved, module, bound.symbol) else {
                continue;
            };
            if !type_satisfies_trait(resolved, layout, types, concrete, trait_def, bound.symbol) {
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

fn resolve_trait_def(
    resolved: &ResolvedProgram,
    module: u32,
    trait_symbol: phx_syntax::Symbol,
) -> Option<DefId> {
    resolved.defs.iter().enumerate().find_map(|(i, d)| {
        if d.kind == DefKind::Trait && d.name == trait_symbol && d.module == module {
            Some(DefId::from_raw(u32::try_from(i).ok()?))
        } else {
            None
        }
    })
}

fn type_satisfies_trait(
    resolved: &ResolvedProgram,
    layout: &ProgramLayout,
    types: &TypeInterner,
    concrete: TypeId,
    trait_def: DefId,
    trait_symbol: phx_syntax::Symbol,
) -> bool {
    if is_copyable_trait_name(resolved, trait_symbol) {
        return is_copyable(types, concrete);
    }
    let Some(concrete_def) = type_def_for_trait_check(types, concrete) else {
        return false;
    };
    layout.trait_impls.contains(&(concrete_def, trait_def))
        || layout
            .trait_methods
            .keys()
            .any(|(type_def, bound_trait, _)| {
                *type_def == concrete_def && *bound_trait == trait_def
            })
}

fn is_copyable_trait_name(resolved: &ResolvedProgram, trait_symbol: phx_syntax::Symbol) -> bool {
    resolved.interner.resolve(trait_symbol) == "Copyable"
}

fn type_def_for_trait_check(types: &TypeInterner, id: TypeId) -> Option<DefId> {
    match types.get(id) {
        Ty::Named { def, .. } => Some(*def),
        Ty::Primitive(_) => None,
        _ => None,
    }
}

fn format_type_name(resolved: &ResolvedProgram, types: &TypeInterner, id: TypeId) -> String {
    super::display::format_type(types, &resolved.interner, &resolved.defs, id)
}

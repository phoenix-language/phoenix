//! Builtin types and Copyable rules.

use phx_syntax::token::Keyword;

use super::layout::{ProgramLayout, TraitInstKey};
use super::std_trait_kernel::StdTraitKernel;
use super::types::{Ty, TypeId, TypeInterner};
use crate::resolver::{DefId, DefKind, ResolvedProgram};

/// Returns the interned unit type.
#[must_use]
pub fn unit(types: &mut TypeInterner) -> TypeId {
    types.intern(&Ty::Unit)
}

/// Returns the interned type for a literal integer with optional `u` suffix.
#[must_use]
pub fn int_literal_type(types: &mut TypeInterner, unsigned: bool) -> TypeId {
    let kw = if unsigned { Keyword::U32 } else { Keyword::S32 };
    types.intern(&Ty::Primitive(kw))
}

/// Returns the interned type for a float literal.
#[must_use]
pub fn float_literal_type(
    types: &mut TypeInterner,
    suffix: phx_syntax::token::FloatSuffix,
) -> TypeId {
    let kw = match suffix {
        phx_syntax::token::FloatSuffix::F64 => Keyword::F64,
        phx_syntax::token::FloatSuffix::None | phx_syntax::token::FloatSuffix::F32 => Keyword::F32,
    };
    types.intern(&Ty::Primitive(kw))
}

/// Returns `bool` type id.
#[must_use]
pub fn bool_type(types: &mut TypeInterner) -> TypeId {
    types.intern(&Ty::Primitive(Keyword::Bool))
}

/// Returns `u8` type id.
#[must_use]
pub fn u8_type(types: &mut TypeInterner) -> TypeId {
    types.intern(&Ty::Primitive(Keyword::U8))
}

/// Returns `str` type id.
#[must_use]
pub fn str_type(types: &mut TypeInterner) -> TypeId {
    types.intern(&Ty::Str)
}

/// Returns whether `id` is Copyable (primitives, unit, str, tuples/arrays of Copyable, eligible structs).
#[must_use]
pub fn is_copyable(
    types: &TypeInterner,
    layout: &ProgramLayout,
    std_traits: &StdTraitKernel,
    id: TypeId,
) -> bool {
    is_copyable_inner(types, layout, std_traits, id, &mut Vec::new())
}

fn is_copyable_inner(
    types: &TypeInterner,
    layout: &ProgramLayout,
    std_traits: &StdTraitKernel,
    id: TypeId,
    seen: &mut Vec<TypeId>,
) -> bool {
    if seen.contains(&id) {
        return false;
    }
    seen.push(id);
    let ok = match types.get(id) {
        Ty::Primitive(_) | Ty::Unit => true,
        Ty::Ref { .. } | Ty::Ptr { .. } | Ty::Var(_) | Ty::Error => false,
        Ty::Fn { .. } => true,
        Ty::Str => true,
        Ty::Slice(inner) => is_copyable_inner(types, layout, std_traits, *inner, seen),
        Ty::Tuple(elems) => elems
            .iter()
            .all(|e| is_copyable_inner(types, layout, std_traits, *e, seen)),
        Ty::Array { elem, .. } => is_copyable_inner(types, layout, std_traits, *elem, seen),
        Ty::Named { def, .. } => {
            if layout.structs.contains_key(def) {
                struct_is_copyable(types, layout, std_traits, *def)
            } else if layout.enums.contains_key(def) {
                enum_is_copyable(types, layout, std_traits, *def)
            } else {
                false
            }
        }
    };
    seen.pop();
    ok
}

fn struct_is_copyable(
    types: &TypeInterner,
    layout: &ProgramLayout,
    std_traits: &StdTraitKernel,
    struct_def: DefId,
) -> bool {
    // `struct_is_copyable` is called without `ResolvedProgram`; std `Drop` id is sufficient
    // for the common case. User-defined `Drop` in the same crate is handled at impl sites.
    if let Some(drop_trait) = std_traits.drop_trait {
        if layout_has_trait_impl(layout, struct_def, &[], drop_trait, &[]) {
            return false;
        }
    }
    let Some(sl) = layout.structs.get(&struct_def) else {
        return false;
    };
    if !sl
        .fields
        .iter()
        .all(|(_, fty)| is_copyable(types, layout, std_traits, *fty))
    {
        return false;
    }
    if let Some(copyable_trait) = std_traits.copyable_trait {
        if layout
            .trait_impls
            .contains(&TraitInstKey::simple(struct_def, copyable_trait))
        {
            return true;
        }
    }
    // Compiler-eligible: all fields Copyable (implicit derived Copyable before explicit impl).
    true
}

fn enum_is_copyable(
    types: &TypeInterner,
    layout: &ProgramLayout,
    std_traits: &StdTraitKernel,
    enum_def: DefId,
) -> bool {
    if let Some(drop_trait) = std_traits.drop_trait {
        if layout_has_trait_impl(layout, enum_def, &[], drop_trait, &[]) {
            return false;
        }
    }
    let Some(el) = layout.enums.get(&enum_def) else {
        return false;
    };
    for variant in &el.variants {
        let payloads_copyable = match &variant.kind {
            crate::typeck::layout::VariantKind::Unit => true,
            crate::typeck::layout::VariantKind::Tuple(ts) => ts
                .iter()
                .all(|fty| is_copyable(types, layout, std_traits, *fty)),
            crate::typeck::layout::VariantKind::Struct(fs) => fs
                .iter()
                .all(|(_, fty)| is_copyable(types, layout, std_traits, *fty)),
        };
        if !payloads_copyable {
            return false;
        }
    }
    if let Some(copyable_trait) = std_traits.copyable_trait {
        if layout
            .trait_impls
            .contains(&TraitInstKey::simple(enum_def, copyable_trait))
        {
            return true;
        }
    }
    true
}

/// Returns whether `trait_def` is a trait named `Copyable`.
#[must_use]
pub fn is_copyable_trait_def(resolved: &ResolvedProgram, trait_def: DefId) -> bool {
    resolved
        .defs
        .get(trait_def.index() as usize)
        .is_some_and(|d| {
            d.kind == DefKind::Trait && resolved.interner.resolve(d.name) == "Copyable"
        })
}

/// Returns whether `trait_def` is a trait named `Drop`.
#[must_use]
pub fn is_drop_trait_def(resolved: &ResolvedProgram, trait_def: DefId) -> bool {
    resolved
        .defs
        .get(trait_def.index() as usize)
        .is_some_and(|d| d.kind == DefKind::Trait && resolved.interner.resolve(d.name) == "Drop")
}

/// Returns whether `def` with `args` has a `Drop` trait impl in `layout`.
#[must_use]
pub fn implements_drop_for_def(
    layout: &ProgramLayout,
    resolved: &ResolvedProgram,
    std_traits: &StdTraitKernel,
    def: DefId,
    args: &[TypeId],
) -> bool {
    if let Some(drop_trait) = std_traits.drop_trait {
        if layout_has_trait_impl(layout, def, args, drop_trait, &[]) {
            return true;
        }
    }
    layout.trait_methods.keys().any(|(key, _)| {
        key.implementer == def
            && key.implementer_args.as_slice() == args
            && is_drop_trait_def(resolved, key.trait_def)
    })
}

/// Returns whether `id` implements `Drop`.
#[must_use]
pub fn implements_drop(
    types: &TypeInterner,
    layout: &ProgramLayout,
    resolved: &ResolvedProgram,
    std_traits: &StdTraitKernel,
    id: TypeId,
) -> bool {
    match types.get(id) {
        Ty::Named { def, args } => {
            implements_drop_for_def(layout, resolved, std_traits, *def, args)
        }
        _ => false,
    }
}

/// Resolves the `Drop::drop` function for a named type instantiation.
#[must_use]
pub fn resolve_drop_fn(
    layout: &ProgramLayout,
    resolved: &ResolvedProgram,
    _std_traits: &StdTraitKernel,
    type_def: DefId,
    type_args: &[TypeId],
) -> Option<DefId> {
    let mut matches: Vec<DefId> = layout
        .trait_methods
        .iter()
        .filter(|((key, _), _)| {
            key.implementer == type_def
                && key.implementer_args.as_slice() == type_args
                && is_drop_trait_def(resolved, key.trait_def)
        })
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

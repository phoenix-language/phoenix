//! Builtin types and Copyable rules.

use phx_syntax::token::Keyword;

use super::layout::ProgramLayout;
use super::std_trait_kernel::StdTraitKernel;
use super::types::{Ty, TypeId, TypeInterner};
use crate::resolver::DefId;

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
        Ty::Ref { .. } | Ty::Ptr { .. } | Ty::Fn { .. } | Ty::Var(_) | Ty::Error => false,
        Ty::Str => true,
        Ty::Slice(inner) => is_copyable_inner(types, layout, std_traits, *inner, seen),
        Ty::Tuple(elems) => elems
            .iter()
            .all(|e| is_copyable_inner(types, layout, std_traits, *e, seen)),
        Ty::Array { elem, .. } => is_copyable_inner(types, layout, std_traits, *elem, seen),
        Ty::Named { def, .. } => struct_is_copyable(types, layout, std_traits, *def),
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
        if layout.trait_impls.contains(&(struct_def, copyable_trait)) {
            return true;
        }
    }
    // Compiler-eligible: all fields Copyable (implicit derived Copyable before explicit impl).
    true
}

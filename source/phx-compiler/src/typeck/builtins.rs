//! Builtin types and Copyable rules.

use phx_syntax::token::Keyword;

use super::types::{Ty, TypeId, TypeInterner};

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

/// Returns whether `id` is Copyable in MVP (primitives, unit, tuples of Copyable, etc.).
#[must_use]
pub fn is_copyable(types: &TypeInterner, id: TypeId) -> bool {
    is_copyable_inner(types, id, &mut Vec::new())
}

fn is_copyable_inner(types: &TypeInterner, id: TypeId, seen: &mut Vec<TypeId>) -> bool {
    if seen.contains(&id) {
        return false;
    }
    seen.push(id);
    let ok = match types.get(id) {
        Ty::Primitive(_) | Ty::Unit => true,
        Ty::Ref { .. }
        | Ty::Ptr { .. }
        | Ty::Fn { .. }
        | Ty::Var(_)
        | Ty::Named { .. }
        | Ty::Error => false,
        Ty::Slice(inner) => is_copyable_inner(types, *inner, seen),
        Ty::Str => true,
        Ty::Tuple(elems) => elems.iter().all(|e| is_copyable_inner(types, *e, seen)),
        Ty::Array { elem, .. } => is_copyable_inner(types, *elem, seen),
    };
    seen.pop();
    ok
}

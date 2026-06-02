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

/// Returns `Option<T>` type id.
#[must_use]
pub fn option_type(types: &mut TypeInterner, inner: TypeId) -> TypeId {
    types.intern(&Ty::Option(inner))
}

/// Returns `Result<T, E>` type id.
#[must_use]
pub fn result_type(types: &mut TypeInterner, ok: TypeId, err: TypeId) -> TypeId {
    types.intern(&Ty::Result { ok, err })
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
        Ty::Ref { .. } | Ty::Ptr { .. } | Ty::Fn { .. } | Ty::Var(_) | Ty::Named { .. } => false,
        Ty::Option(inner) | Ty::Slice(inner) => is_copyable_inner(types, *inner, seen),
        Ty::Result { ok, err } => {
            is_copyable_inner(types, *ok, seen) && is_copyable_inner(types, *err, seen)
        }
        Ty::Tuple(elems) => elems.iter().all(|e| is_copyable_inner(types, *e, seen)),
        Ty::Array { elem, .. } => is_copyable_inner(types, *elem, seen),
    };
    seen.pop();
    ok
}

/// Returns `true` if `id` is `Result<_, _>` or `Option<_>`.
#[must_use]
pub fn is_result_or_option(types: &TypeInterner, id: TypeId) -> bool {
    matches!(types.get(id), Ty::Result { .. } | Ty::Option(_))
}

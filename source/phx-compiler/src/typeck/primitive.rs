//! Maps typeck primitives to bytecode cast operands.

use phx_bytecode::PrimitiveKind;
use phx_syntax::token::Keyword;

use super::types::{Ty, TypeId, TypeInterner};

/// Returns wire cast kind for a primitive type id.
#[must_use]
pub fn primitive_kind_for_type(types: &TypeInterner, ty: TypeId) -> Option<PrimitiveKind> {
    match types.get(ty) {
        Ty::Primitive(kw) => keyword_to_primitive_kind(*kw),
        _ => None,
    }
}

/// Maps a Phoenix primitive keyword to a cast kind (MVP int/bool only).
#[must_use]
pub fn keyword_to_primitive_kind(kw: Keyword) -> Option<PrimitiveKind> {
    Some(match kw {
        Keyword::S8 => PrimitiveKind::S8,
        Keyword::S16 => PrimitiveKind::S16,
        Keyword::S32 => PrimitiveKind::S32,
        Keyword::S64 => PrimitiveKind::S64,
        Keyword::S128 => PrimitiveKind::S128,
        Keyword::U8 => PrimitiveKind::U8,
        Keyword::U16 => PrimitiveKind::U16,
        Keyword::U32 => PrimitiveKind::U32,
        Keyword::U64 => PrimitiveKind::U64,
        Keyword::U128 => PrimitiveKind::U128,
        Keyword::Bool => PrimitiveKind::Bool,
        Keyword::F32 | Keyword::F64 => return None,
        _ => return None,
    })
}

/// Returns `true` when `kw` is an MVP integer primitive (signed or unsigned).
#[must_use]
pub fn is_int_keyword(kw: Keyword) -> bool {
    matches!(
        kw,
        Keyword::S8
            | Keyword::S16
            | Keyword::S32
            | Keyword::S64
            | Keyword::S128
            | Keyword::U8
            | Keyword::U16
            | Keyword::U32
            | Keyword::U64
            | Keyword::U128
    )
}

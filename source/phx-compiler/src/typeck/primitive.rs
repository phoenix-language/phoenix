//! Maps typeck primitives to bytecode cast operands.

use phx_bytecode::{LocalSlotKind, PrimitiveKind};
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
        Keyword::F32 => PrimitiveKind::F32,
        Keyword::F64 => PrimitiveKind::F64,
        _ => return None,
    })
}

/// Byte size for pointer load/store of a primitive (`s128`/`u128` use 8 bytes in MVP VM).
#[allow(dead_code)]
#[must_use]
pub fn primitive_byte_size(kind: PrimitiveKind) -> u8 {
    match kind {
        PrimitiveKind::S8 | PrimitiveKind::U8 | PrimitiveKind::Bool => 1,
        PrimitiveKind::S16 | PrimitiveKind::U16 => 2,
        PrimitiveKind::S32 | PrimitiveKind::U32 | PrimitiveKind::F32 => 4,
        PrimitiveKind::S64
        | PrimitiveKind::U64
        | PrimitiveKind::F64
        | PrimitiveKind::S128
        | PrimitiveKind::U128 => 8,
    }
}

/// Returns `1` when the primitive is signed integer (not float/bool).
#[must_use]
pub fn primitive_load_signed(kind: PrimitiveKind) -> u8 {
    match kind {
        PrimitiveKind::Bool
        | PrimitiveKind::U8
        | PrimitiveKind::U16
        | PrimitiveKind::U32
        | PrimitiveKind::U64
        | PrimitiveKind::U128
        | PrimitiveKind::F32
        | PrimitiveKind::F64 => 0,
        _ => 1,
    }
}

/// Maps a binding type to a bytecode local slot kind.
#[must_use]
pub fn slot_kind_for_binding(types: &TypeInterner, ty: TypeId) -> LocalSlotKind {
    if matches!(types.get(ty), Ty::Fn { .. }) {
        LocalSlotKind::fn_ptr()
    } else if let Some(kind) = primitive_kind_for_type(types, ty) {
        LocalSlotKind::primitive(kind)
    } else {
        LocalSlotKind::aggregate()
    }
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

//! Maps typeck primitives to bytecode cast operands.

use phx_bytecode::{LocalSlotKind, PrimitiveKind};
use phx_syntax::Interner;
use phx_syntax::Symbol;
use phx_syntax::ast::{ImplMember, TopLevelDecl};
use phx_syntax::token::Keyword;

use crate::resolver::{DefId, DefKind, ResolutionKey, ResolvedProgram};
use crate::typeck::TypedProgram;

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
    } else if matches!(types.get(ty), Ty::Ptr { .. } | Ty::Ref { .. }) {
        LocalSlotKind::primitive(PrimitiveKind::U64)
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

/// Primitive keywords allowed as `s32 :: impl :: Trait` receivers.
const IMPL_PRIMITIVE_KEYWORDS: [Keyword; 11] = [
    Keyword::Bool,
    Keyword::S8,
    Keyword::S16,
    Keyword::S32,
    Keyword::S64,
    Keyword::U8,
    Keyword::U16,
    Keyword::U32,
    Keyword::U64,
    Keyword::F32,
    Keyword::F64,
];

/// Returns the primitive keyword when `symbol` names a builtin impl receiver (`s32`, `bool`, …).
#[must_use]
pub fn keyword_for_impl_type_symbol(interner: &Interner, symbol: Symbol) -> Option<Keyword> {
    IMPL_PRIMITIVE_KEYWORDS
        .into_iter()
        .find(|kw| interner.resolves_to(symbol, keyword_type_name(*kw)))
}

/// Returns `true` when `symbol` is the language `str` type name.
#[must_use]
pub fn is_str_impl_type_symbol(interner: &Interner, symbol: Symbol) -> bool {
    interner.resolves_to(symbol, "str")
}

/// Returns `true` when `fn_def` is a trait method on a builtin primitive or `str` receiver.
#[must_use]
pub fn is_builtin_type_impl_method(typed: &TypedProgram, fn_def: DefId) -> bool {
    let Some(base_def) = typed.resolved.defs.get(fn_def.index() as usize) else {
        return false;
    };
    if base_def.kind != DefKind::ImplMethod {
        return false;
    }
    builtin_impl_receiver_for_method(&typed.resolved, base_def.module, fn_def)
}

/// Returns `true` when `fn_def` is `str`'s trait method (`str :: impl :: Trait { fmt … }`).
#[must_use]
pub fn is_str_builtin_impl_method(typed: &TypedProgram, fn_def: DefId) -> bool {
    let Some(base_def) = typed.resolved.defs.get(fn_def.index() as usize) else {
        return false;
    };
    if base_def.kind != DefKind::ImplMethod {
        return false;
    }
    str_impl_method_receiver(&typed.resolved, base_def.module, fn_def)
}

fn builtin_impl_receiver_for_method(
    resolved: &ResolvedProgram,
    module: u32,
    fn_def: DefId,
) -> bool {
    str_impl_method_receiver(resolved, module, fn_def)
        || other_primitive_impl_method_receiver(resolved, module, fn_def)
}

fn str_impl_method_receiver(resolved: &ResolvedProgram, module: u32, fn_def: DefId) -> bool {
    impl_method_on_type_name(resolved, module, fn_def, |interner, sym| {
        is_str_impl_type_symbol(interner, sym)
    })
}

fn other_primitive_impl_method_receiver(
    resolved: &ResolvedProgram,
    module: u32,
    fn_def: DefId,
) -> bool {
    impl_method_on_type_name(resolved, module, fn_def, |interner, sym| {
        keyword_for_impl_type_symbol(interner, sym).is_some()
    })
}

fn impl_method_on_type_name(
    resolved: &ResolvedProgram,
    module: u32,
    fn_def: DefId,
    type_matches: impl Fn(&Interner, Symbol) -> bool,
) -> bool {
    let Some(base_def) = resolved.defs.get(fn_def.index() as usize) else {
        return false;
    };
    let interner = &resolved.interner;
    for m in &resolved.modules {
        if m.id != module {
            continue;
        }
        for item in &m.program.items {
            let TopLevelDecl::Impl {
                type_name, members, ..
            } = &item.inner.decl
            else {
                continue;
            };
            let has_method = members.iter().any(|member| {
                let ImplMember::Method(f) = member else {
                    return false;
                };
                if f.name.symbol != base_def.name {
                    return false;
                }
                resolved
                    .resolutions
                    .get(&ResolutionKey {
                        module,
                        node_id: f.name.id,
                    })
                    .copied()
                    .is_some_and(|d| d == fn_def)
            });
            if !has_method {
                continue;
            }
            return type_matches(interner, type_name.symbol);
        }
    }
    false
}

fn keyword_type_name(kw: Keyword) -> &'static str {
    match kw {
        Keyword::Bool => "bool",
        Keyword::S8 => "s8",
        Keyword::S16 => "s16",
        Keyword::S32 => "s32",
        Keyword::S64 => "s64",
        Keyword::U8 => "u8",
        Keyword::U16 => "u16",
        Keyword::U32 => "u32",
        Keyword::U64 => "u64",
        Keyword::F32 => "f32",
        Keyword::F64 => "f64",
        _ => "",
    }
}

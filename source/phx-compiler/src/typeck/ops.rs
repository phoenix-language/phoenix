//! Operator typing for MVP primitives.

use phx_syntax::token::Keyword;

use super::types::{Ty, TypeId, TypeInterner};
use super::unify::{AliasEnv, normalize_type};
use crate::typeck::builtins::bool_type;

/// Result of checking a binary operator.
pub struct BinOpResult {
    /// Result type of the operation.
    pub result: TypeId,
}

/// Returns result type for `op` on `lhs` and `rhs`, or `None` if invalid.
#[must_use]
pub fn check_binary(
    types: &mut TypeInterner,
    op: phx_syntax::ast::expr::BinOp,
    lhs: TypeId,
    rhs: TypeId,
) -> Option<BinOpResult> {
    if lhs != rhs {
        return None;
    }
    let ty = types.get(lhs);
    match op {
        phx_syntax::ast::expr::BinOp::Or | phx_syntax::ast::expr::BinOp::And => {
            if matches!(ty, Ty::Primitive(Keyword::Bool)) {
                Some(BinOpResult {
                    result: bool_type(types),
                })
            } else {
                None
            }
        }
        phx_syntax::ast::expr::BinOp::Eq
        | phx_syntax::ast::expr::BinOp::Ne
        | phx_syntax::ast::expr::BinOp::Lt
        | phx_syntax::ast::expr::BinOp::Le
        | phx_syntax::ast::expr::BinOp::Gt
        | phx_syntax::ast::expr::BinOp::Ge => {
            if is_primitive(ty) {
                Some(BinOpResult {
                    result: bool_type(types),
                })
            } else {
                None
            }
        }
        phx_syntax::ast::expr::BinOp::Add
        | phx_syntax::ast::expr::BinOp::Sub
        | phx_syntax::ast::expr::BinOp::Mul
        | phx_syntax::ast::expr::BinOp::Div => {
            if is_numeric_primitive(ty) {
                Some(BinOpResult { result: lhs })
            } else {
                None
            }
        }
        phx_syntax::ast::expr::BinOp::Mod | phx_syntax::ast::expr::BinOp::Pow => {
            if is_int_numeric_primitive(ty) {
                Some(BinOpResult { result: lhs })
            } else {
                None
            }
        }
        phx_syntax::ast::expr::BinOp::BitOr
        | phx_syntax::ast::expr::BinOp::BitXor
        | phx_syntax::ast::expr::BinOp::BitAnd
        | phx_syntax::ast::expr::BinOp::Shl
        | phx_syntax::ast::expr::BinOp::Shr => {
            if is_int_numeric_primitive(ty) {
                Some(BinOpResult { result: lhs })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Returns result type for unary `op` on `operand`.
#[must_use]
pub fn check_unary(
    types: &mut TypeInterner,
    op: phx_syntax::ast::expr::UnaryOp,
    operand: TypeId,
) -> Option<TypeId> {
    let ty = types.get(operand);
    match op {
        phx_syntax::ast::expr::UnaryOp::Neg => {
            if is_numeric_primitive(ty) {
                Some(operand)
            } else {
                None
            }
        }
        phx_syntax::ast::expr::UnaryOp::BitNot => {
            if is_int_numeric_primitive(ty) {
                Some(operand)
            } else {
                None
            }
        }
        phx_syntax::ast::expr::UnaryOp::Not => {
            if matches!(ty, Ty::Primitive(Keyword::Bool)) {
                Some(bool_type(types))
            } else {
                None
            }
        }
        phx_syntax::ast::expr::UnaryOp::Deref => {
            if let Ty::Ref { inner, .. } | Ty::Ptr { inner, .. } = ty {
                Some(*inner)
            } else {
                None
            }
        }
        phx_syntax::ast::expr::UnaryOp::Ref | phx_syntax::ast::expr::UnaryOp::RefMut => {
            if matches!(ty, Ty::Named { .. } | Ty::Primitive(_) | Ty::Array { .. }) {
                Some(types.intern(&Ty::Ref {
                    mut_: matches!(op, phx_syntax::ast::expr::UnaryOp::RefMut),
                    inner: operand,
                }))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Returns `true` when `from` may be cast to `to` explicitly (MVP Tier A casts).
///
/// Numeric casts allow cross-width and signed/unsigned/float combinations via explicit `as`;
/// `bool` is excluded. View casts: array→slice, str→`[u8]`. There is no implicit widening.
#[must_use]
pub fn check_cast(env: &AliasEnv<'_>, from: TypeId, to: TypeId) -> bool {
    let from = normalize_type(env, from);
    let to = normalize_type(env, to);
    let types = env.types;
    let f = types.get(from);
    let t = types.get(to);
    match (f, t) {
        (Ty::Primitive(a), Ty::Primitive(b)) => primitive_cast_allowed(*a, *b),
        (Ty::Array { elem, .. }, Ty::Slice(slice_elem)) => *elem == *slice_elem,
        (Ty::Str, Ty::Slice(slice_elem)) => {
            matches!(types.get(*slice_elem), Ty::Primitive(Keyword::U8))
        }
        (
            Ty::Ptr {
                inner: from_inner, ..
            },
            Ty::Ptr {
                inner: to_inner, ..
            },
        ) => ptr_elem_cast_allowed(types, *from_inner, *to_inner),
        _ => from == to,
    }
}

fn ptr_elem_cast_allowed(types: &TypeInterner, from: TypeId, to: TypeId) -> bool {
    if from == to {
        return true;
    }
    let from_ty = types.get(from);
    let to_ty = types.get(to);
    matches!(
        (from_ty, to_ty),
        (Ty::Primitive(Keyword::U8), Ty::Primitive(_))
            | (Ty::Primitive(_), Ty::Primitive(Keyword::U8))
    )
}

fn is_primitive(ty: &Ty) -> bool {
    matches!(ty, Ty::Primitive(_))
}

fn is_numeric_primitive(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Primitive(
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
                | Keyword::F32
                | Keyword::F64
        )
    )
}

fn primitive_cast_allowed(from: Keyword, to: Keyword) -> bool {
    if from == to {
        return true;
    }
    if matches!(from, Keyword::Bool) || matches!(to, Keyword::Bool) {
        return false;
    }
    let from_int = is_int_keyword(from);
    let to_int = is_int_keyword(to);
    let from_float = matches!(from, Keyword::F32 | Keyword::F64);
    let to_float = matches!(to, Keyword::F32 | Keyword::F64);
    (from_int || from_float) && (to_int || to_float)
}

fn is_int_numeric_primitive(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Primitive(
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
    )
}

fn is_int_keyword(kw: Keyword) -> bool {
    crate::typeck::primitive::is_int_keyword(kw)
}

//! Operator and cast typing for MVP primitives.
//!
//! Assigns result types for binary and unary operators and validates explicit `as` casts on
//! interned primitive and pointer types. Called from [`super::check`] during expression typing;
//! does not emit diagnostics — callers report errors when these helpers return `None` or
//! `false`.
//!
//! # Role in type checking
//!
//! Encodes Phoenix's Tier-A operator rules: operands must agree in type (no implicit numeric
//! widening), comparisons on primitives and pointers produce `bool`, arithmetic preserves the
//! operand type, and bitwise operators require integer operands. Reference operators (`&`,
//! `&mut`, `*`) construct or peel [`Ty::Ref`] / [`Ty::Ptr`] as appropriate.
//!
//! # Binary operators
//!
//! [`check_binary`] requires `lhs == rhs` before inspecting the operator. Logical `&&`/`||`
//! require `bool`; comparisons accept primitives, pointers, and named types; arithmetic
//! requires numeric primitives; `%` and bitwise ops require integer primitives; `**` (pow) is
//! not yet supported and always returns `None`.
//!
//! # Unary operators
//!
//! [`check_unary`] handles negation and bitwise not on numeric/integer types, logical not on
//! `bool`, dereference of references and pointers, and address-of (`&` / `&mut`) on named
//! types, primitives, and arrays.
//!
//! # Explicit casts
//!
//! [`check_cast`] validates `as` conversions under alias normalization via [`super::unify::AliasEnv`]:
//!
//! - Numeric primitives may cast across width and signed/unsigned/float combinations except
//!   involving `bool`.
//! - `[T; N]` may cast to `[T]` (array to slice view).
//! - `str` may cast to `[u8]` (byte view).
//! - Pointer element types may cast when at least one side is `u8`.
//! - Identical types always succeed.

use phx_syntax::token::Keyword;

use super::types::{Ty, TypeId, TypeInterner};
use super::unify::{AliasEnv, normalize_type};
use crate::typeck::builtins::bool_type;

/// Result of successfully checking a binary operator.
pub struct BinOpResult {
    /// Interned result type of the operation (for example `bool` for comparisons, operand type
    /// for arithmetic).
    pub result: TypeId,
}

/// Returns the result type for binary `op` on `lhs` and `rhs`.
///
/// Returns `None` when operands differ, the operator is unsupported for the operand type, or
/// the operator has no MVP typing rule (for example [`phx_syntax::ast::expr::BinOp::Pow`]).
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
            if is_primitive(ty) || matches!(ty, Ty::Ptr { .. } | Ty::Named { .. }) {
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
        phx_syntax::ast::expr::BinOp::Mod => {
            if is_int_numeric_primitive(ty) {
                Some(BinOpResult { result: lhs })
            } else {
                None
            }
        }
        phx_syntax::ast::expr::BinOp::Pow => None,
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
    }
}

/// Returns the result type for unary `op` applied to `operand`.
///
/// Returns `None` when the operator is invalid for the operand type (for example negation on
/// `bool`, or address-of on a reference).
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
    }
}

/// Returns `true` when an explicit `as` cast from `from` to `to` is allowed.
///
/// Types are normalized through type aliases before comparison. See the
/// [module-level cast rules](self#explicit-casts) for supported conversions. There is no
/// implicit widening — this helper is only for explicit `as` expressions.
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

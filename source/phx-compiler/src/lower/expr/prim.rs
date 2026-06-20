//! Primitive kind resolution when typeck expression types are still generic.
//!
//! After monomorphization some expression nodes retain unresolved or parametric types; bytecode
//! opcodes still need a concrete [`PrimitiveKind`](phx_bytecode::PrimitiveKind) wire tag. This
//! module inspects operand shape (literals, bindings, single field chains) to pick a kind when
//! [`primitive_kind_for_type`](crate::typeck::primitive_kind_for_type) returns `None`.
//!
//! [`prim_kind_for_binop`] is used by [`super::literal::lower_binary`];
//! [`pointee_type_for_deref_operand`] supports pointer deref in [`super::lower_expr_inner`].

use phx_syntax::Symbol;
use phx_syntax::ast::expr::{Expr, ExprNode, PostfixOp};
use phx_syntax::ast::ident::Ident;
use phx_syntax::ast::lit::Literal;
use phx_syntax::token::IntegerSuffix;

use crate::lower::ctx::{LowerCtx, prim_kind_byte};
use crate::resolver::DefId;
use crate::typeck::{Ty, TypeId, primitive_kind_for_type};
use phx_bytecode::PrimitiveKind;

/// Wire primitive kind for a binary op, falling back to operand shapes when `result_ty` is generic.
pub(super) fn prim_kind_for_binop(
    ctx: &LowerCtx<'_>,
    result_ty: TypeId,
    left: &ExprNode,
    right: &ExprNode,
) -> u8 {
    if let Some(kind) = primitive_kind_for_type(&ctx.typed.types, result_ty) {
        return kind.as_u8();
    }
    prim_kind_from_expr_operand(ctx, left)
        .or_else(|| prim_kind_from_expr_operand(ctx, right))
        .unwrap_or_else(|| prim_kind_byte(ctx.typed, result_ty))
}

fn prim_kind_from_expr_operand(ctx: &LowerCtx<'_>, expr: &ExprNode) -> Option<u8> {
    match &expr.inner {
        Expr::Ident(ident) => prim_kind_from_ident(ctx, *ident),
        Expr::Literal(lit) => literal_wire_kind(lit),
        Expr::Postfix { base, ops } if ops.len() == 1 => {
            let PostfixOp::Field(field) = &ops[0] else {
                return None;
            };
            prim_kind_from_field(ctx, base, field.symbol)
        }
        _ => None,
    }
}

fn prim_kind_from_ident(ctx: &LowerCtx<'_>, ident: Ident) -> Option<u8> {
    let binding = ctx.layout.binding(ident.symbol)?;
    primitive_kind_for_type(&ctx.typed.types, binding.ty).map(PrimitiveKind::as_u8)
}

fn prim_kind_from_field(ctx: &LowerCtx<'_>, base: &ExprNode, field: Symbol) -> Option<u8> {
    let (def, args) = named_parts_from_base(ctx, base)?;
    let idx = ctx.typed.layout.struct_field_index(def, field, &args)?;
    let sl = ctx.typed.layout.struct_layout(def, &args)?;
    let fty = sl.fields.get(idx as usize)?.1;
    primitive_kind_for_type(&ctx.typed.types, fty).map(PrimitiveKind::as_u8)
}

fn named_parts_from_base(ctx: &LowerCtx<'_>, base: &ExprNode) -> Option<(DefId, Vec<TypeId>)> {
    let Expr::Ident(ident) = &base.inner else {
        return None;
    };
    let binding = ctx.layout.binding(ident.symbol)?;
    named_parts_from_ty(ctx, binding.ty)
}

fn named_parts_from_ty(ctx: &LowerCtx<'_>, ty: TypeId) -> Option<(DefId, Vec<TypeId>)> {
    let inner = match ctx.typed.types.get(ty) {
        Ty::Ref { inner, .. } => *inner,
        _ => ty,
    };
    match ctx.typed.types.get(inner) {
        Ty::Named { def, args } => Some((*def, args.clone())),
        _ => None,
    }
}

/// Concrete pointee type for `*expr` when expr types are still generic after monomorphization.
pub(super) fn pointee_type_for_deref_operand(
    ctx: &LowerCtx<'_>,
    operand: &ExprNode,
    operand_ty: TypeId,
) -> Option<TypeId> {
    if let Ty::Ptr { inner, .. } = ctx.typed.types.get(operand_ty) {
        if primitive_kind_for_type(&ctx.typed.types, *inner).is_some() {
            return Some(*inner);
        }
    }
    match &operand.inner {
        Expr::Postfix { base, ops } if !ops.is_empty() => {
            let PostfixOp::Field(field) = ops.last()? else {
                return None;
            };
            let (def, args) = named_parts_from_base(ctx, base)?;
            let idx = ctx
                .typed
                .layout
                .struct_field_index(def, field.symbol, &args)?;
            let fty = ctx
                .typed
                .layout
                .struct_layout(def, &args)?
                .fields
                .get(idx as usize)?
                .1;
            match ctx.typed.types.get(fty) {
                Ty::Ptr { inner, .. } => Some(*inner),
                _ => primitive_kind_for_type(&ctx.typed.types, fty).map(|_| fty),
            }
        }
        _ => None,
    }
}

fn literal_wire_kind(lit: &Literal) -> Option<u8> {
    match lit {
        Literal::Int(i) => {
            if i.suffix == IntegerSuffix::Unsigned {
                Some(PrimitiveKind::U32.as_u8())
            } else {
                Some(PrimitiveKind::S32.as_u8())
            }
        }
        Literal::Float(_) => Some(PrimitiveKind::F64.as_u8()),
        Literal::Bool(_) => Some(PrimitiveKind::Bool.as_u8()),
        _ => None,
    }
}

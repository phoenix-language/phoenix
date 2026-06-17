//! Assignment and compound store lowering.

use phx_syntax::ast::expr::{Expr, ExprNode, PostfixOp, UnaryOp};

use crate::ir::IrInst;
use crate::lower::ctx::{LowerCtx, prim_kind_byte};
use crate::typeck::{Ty, TypeId, primitive_kind_for_type, primitive_load_signed};
use phx_bytecode::PrimitiveKind;

use super::call::{
    binding_is_ref_to_struct, emit_load_struct_base, store_base_local, struct_args_from_base,
    struct_def_from_base,
};
use super::{lower_expr, lower_expr_typed, lower_expr_with_type};

/// Type of the slice/base for `base[index] = …` (when `target` is a single index postfix).
fn slice_ty_for_index_assign_target(ctx: &LowerCtx<'_>, target: &Expr) -> Option<TypeId> {
    let Expr::Postfix { base, ops, .. } = target else {
        return None;
    };
    if ops.len() != 1 || !matches!(ops[0], PostfixOp::Index(_)) {
        return None;
    }
    type_id_for_index_base(ctx, base)
}

fn type_id_for_index_base(ctx: &LowerCtx<'_>, base: &ExprNode) -> Option<TypeId> {
    match &base.inner {
        Expr::Ident(ident) => ctx.layout.binding(ident.symbol).map(|b| b.ty),
        _ => None,
    }
}

fn indexable_element_ty(ctx: &LowerCtx<'_>, base_ty: TypeId) -> Option<TypeId> {
    match ctx.typed.types.get(base_ty) {
        Ty::Ref { inner, .. } => indexable_element_ty(ctx, *inner),
        Ty::Slice(elem) | Ty::Array { elem, .. } => Some(*elem),
        _ => None,
    }
}

fn prim_kind_for_index_store(
    ctx: &LowerCtx<'_>,
    value_ty: TypeId,
    target: &Expr,
) -> Option<PrimitiveKind> {
    primitive_kind_for_type(&ctx.typed.types, value_ty).or_else(|| {
        slice_ty_for_index_assign_target(ctx, target)
            .and_then(|base_ty| indexable_element_ty(ctx, base_ty))
            .and_then(|elem| primitive_kind_for_type(&ctx.typed.types, elem))
    })
}

/// Primitive kind for a store RHS, falling back to layout binding type for simple idents.
fn prim_kind_for_assign_value(
    ctx: &LowerCtx<'_>,
    value: &ExprNode,
    value_ty: TypeId,
) -> Option<PrimitiveKind> {
    primitive_kind_for_type(&ctx.typed.types, value_ty).or_else(|| {
        if let Expr::Ident(ident) = &value.inner {
            ctx.layout
                .binding(ident.symbol)
                .and_then(|b| primitive_kind_for_type(&ctx.typed.types, b.ty))
        } else {
            None
        }
    })
}

/// Lowers assignment (`=`, `+=`, …) into store or compound-op IR.
pub(crate) fn lower_assign_expr(
    ctx: &mut LowerCtx<'_>,
    target: &ExprNode,
    value: &ExprNode,
    _result_ty: TypeId,
) {
    lower_assign_target(ctx, &target.inner);
    let value_ty = if let Some(elem_ty) = slice_ty_for_index_assign_target(ctx, &target.inner)
        .and_then(|base_ty| indexable_element_ty(ctx, base_ty))
    {
        lower_expr_with_type(ctx, value, elem_ty);
        elem_ty
    } else {
        lower_expr_typed(ctx, value)
    };
    match &target.inner {
        Expr::Ident(ident) => {
            if let Some(binding) = ctx.layout.binding(ident.symbol) {
                ctx.emit_here(IrInst::StoreLocal {
                    slot: binding.slot,
                    ty: binding.ty,
                    prim_kind: prim_kind_byte(ctx.typed, binding.ty),
                });
            }
        }
        Expr::Unary {
            op: UnaryOp::Deref, ..
        } => {
            if let Some(kind) = prim_kind_for_assign_value(ctx, value, value_ty) {
                ctx.emit_here(IrInst::PtrStore {
                    prim_kind: kind.as_u8(),
                    signed: primitive_load_signed(kind),
                });
            }
        }
        Expr::Postfix { base, ops } if ops.len() == 1 => {
            if let PostfixOp::Field(field) = &ops[0] {
                if let Some(def) = struct_def_from_base(ctx, base) {
                    let args = struct_args_from_base(ctx, base);
                    let type_id = ctx.typed.layout.type_id_for_named(def, &args).unwrap_or(0);
                    let field_index = ctx
                        .typed
                        .layout
                        .struct_field_index(def, field.symbol, &args)
                        .unwrap_or(0);
                    ctx.emit_here(IrInst::SetField {
                        type_id,
                        field_index,
                    });
                    if let Expr::Ident(ident) = &base.inner {
                        if binding_is_ref_to_struct(ctx, ident.symbol) {
                            ctx.emit_here(IrInst::Pop);
                        } else {
                            store_base_local(ctx, base);
                        }
                    } else {
                        store_base_local(ctx, base);
                    }
                }
            } else if matches!(ops[0], PostfixOp::Index(_)) {
                if let Some(kind) = prim_kind_for_index_store(ctx, value_ty, &target.inner) {
                    ctx.emit_here(IrInst::IndexStore {
                        prim_kind: kind.as_u8(),
                        signed: primitive_load_signed(kind),
                    });
                }
            }
        }
        _ => {}
    }
}

fn lower_assign_target(ctx: &mut LowerCtx<'_>, target: &Expr) {
    match target {
        Expr::Ident(_) => {}
        Expr::Unary {
            op: UnaryOp::Deref,
            operand,
            ..
        } => {
            lower_expr(ctx, operand);
        }
        Expr::Postfix { base, ops } if ops.len() == 1 => match &ops[0] {
            PostfixOp::Field(_) => emit_load_struct_base(ctx, base),
            PostfixOp::Index(idx) => {
                lower_expr(ctx, base);
                lower_expr(ctx, idx);
            }
            _ => {}
        },
        Expr::Literal(_) | Expr::Path(_) => {}
        _ => {}
    }
}

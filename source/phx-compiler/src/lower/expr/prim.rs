//! Primitive wire-kind and pointee-type recovery during expression lowering.
//!
//! **Pipeline position:** [`super::lower_expr_inner`] and [`super::literal::lower_binary`], after
//! typeck and monomorphization, before codegen. Does not read source text or mutate the AST.
//!
//! **Inputs:** [`LowerCtx`] (typed types, layout bindings, struct field metadata), operand
//! [`ExprNode`] shapes, and typeck-assigned [`TypeId`]s — often still generic or parametric after
//! specialization.
//!
//! **Outputs:** `u8` wire tags for [`IrInst::BinOp`](crate::ir::IrInst::BinOp),
//! [`IrInst::PtrLoad`](crate::ir::IrInst::PtrLoad), and related stack instructions; optional
//! concrete pointee [`TypeId`] for pointer dereference.
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

/// Resolves the wire [`PrimitiveKind`](phx_bytecode::PrimitiveKind) byte for [`IrInst::BinOp`](crate::ir::IrInst::BinOp).
///
/// Uses `result_ty` when typeck assigned a concrete primitive. After monomorphization the result
/// type may still be generic; in that case inspects `left` and `right` operand shapes (literals,
/// bindings, single trailing field access) before falling back to
/// [`prim_kind_byte`](crate::lower::ctx::prim_kind_byte).
///
/// Called from [`super::literal::lower_binary`] after both operands are evaluated onto the stack.
///
/// # Panics
///
/// Never panics on malformed user input; returns a conservative kind (often aggregate
/// [`SLOT_KIND_AGG`](phx_bytecode::SLOT_KIND_AGG)) when no concrete primitive can be inferred.
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

/// Infers a wire kind from a single operand expression when its type is still generic.
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

/// Wire kind from a binding's declared type when the ident is in scope layout.
fn prim_kind_from_ident(ctx: &LowerCtx<'_>, ident: Ident) -> Option<u8> {
    let binding = ctx.layout.binding(ident.symbol)?;
    primitive_kind_for_type(&ctx.typed.types, binding.ty).map(PrimitiveKind::as_u8)
}

/// Wire kind from one struct field when `base` names the struct and layout metadata is available.
fn prim_kind_from_field(ctx: &LowerCtx<'_>, base: &ExprNode, field: Symbol) -> Option<u8> {
    let (def, args) = named_parts_from_base(ctx, base)?;
    let idx = ctx.typed.layout.struct_field_index(def, field, &args)?;
    let sl = ctx.typed.layout.struct_layout(def, &args)?;
    let fty = sl.fields.get(idx as usize)?.1;
    primitive_kind_for_type(&ctx.typed.types, fty).map(PrimitiveKind::as_u8)
}

/// `(DefId, type args)` when `base` is an ident bound to a named (possibly ref-wrapped) type.
fn named_parts_from_base(ctx: &LowerCtx<'_>, base: &ExprNode) -> Option<(DefId, Vec<TypeId>)> {
    let Expr::Ident(ident) = &base.inner else {
        return None;
    };
    let binding = ctx.layout.binding(ident.symbol)?;
    named_parts_from_ty(ctx, binding.ty)
}

/// Strips one reference layer and extracts named-type head plus specialization args.
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

/// Concrete pointee type for unary `*operand` when `operand_ty` is still generic.
///
/// Unwraps `Ty::Ptr` when the inner type has a concrete primitive kind. Otherwise walks a single
/// trailing field postfix on `operand` to recover the field's pointer or primitive type from
/// [`TypedProgram::layout`](crate::typeck::TypedProgram::layout) struct metadata.
///
/// Used by [`super::lower_expr_inner`] before emitting [`IrInst::PtrLoad`](crate::ir::IrInst::PtrLoad).
///
/// # Panics
///
/// Never panics; returns `None` when the pointee cannot be determined from type or operand shape.
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

/// Wire kind implied by a literal token when the literal appears as a binop operand.
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

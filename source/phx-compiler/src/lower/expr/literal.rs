//! Literals, identifiers, paths, and binary operators.

use phx_syntax::ast::expr::{BinOp, Expr, ExprNode};
use phx_syntax::ast::ident::{Ident, Path, PathSegment};
use phx_syntax::ast::lit::Literal;
use phx_syntax::token::IntegerSuffix;

use crate::ir::IrConst;
use crate::ir::{IrBinOp, IrInst};
use crate::lower::ctx::{LowerCtx, bool_ty, fn_ptr_target, lookup_resolution, prim_kind_byte};
use crate::typeck::{BindingKind, Ty, TypeId, primitive_kind_for_type};
use phx_bytecode::{PrimitiveKind, SLOT_KIND_AGG, ScalarValue};

use super::lower_expr;

pub(super) fn intern_literal(ctx: &mut LowerCtx<'_>, lit: &Literal, ty: TypeId) -> u32 {
    match lit {
        Literal::Int(i) => {
            let from = if i.suffix == IntegerSuffix::Unsigned {
                PrimitiveKind::U32
            } else {
                PrimitiveKind::S32
            };
            let to = primitive_kind_for_type(&ctx.typed.types, ty).unwrap_or(from);
            let raw = i.value;
            #[allow(clippy::cast_possible_truncation)]
            let stored = PrimitiveKind::apply_cast(ScalarValue::I32(raw as i32), from, to);
            let (value, kind) = scalar_to_ir_const(stored, to);
            ctx.intern_const(IrConst::Int(value, kind))
        }
        Literal::Float(f) => {
            let kind = primitive_kind_for_type(&ctx.typed.types, ty).unwrap_or(PrimitiveKind::F64);
            ctx.intern_const(IrConst::Float(f.value, kind))
        }
        Literal::Bool(b) => ctx.intern_const(IrConst::Bool(*b)),
        Literal::ByteChar(c) => {
            let u8_ty = primitive_kind_for_type(&ctx.typed.types, ty).unwrap_or(PrimitiveKind::U8);
            ctx.intern_const(IrConst::Int(i128::from(*c), u8_ty))
        }
        Literal::ByteString(b) => ctx.intern_const(IrConst::Bytes(b.clone())),
        Literal::String(s) => ctx.intern_const(IrConst::Bytes(s.clone().into_bytes())),
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]
fn scalar_to_ir_const(v: ScalarValue, kind: PrimitiveKind) -> (i128, PrimitiveKind) {
    let n = match v {
        ScalarValue::I8(x) => i128::from(x),
        ScalarValue::I16(x) => i128::from(x),
        ScalarValue::I32(x) => i128::from(x),
        ScalarValue::I64(x) => i128::from(x),
        ScalarValue::I128(x) => x,
        ScalarValue::U8(x) => i128::from(x),
        ScalarValue::U16(x) => i128::from(x),
        ScalarValue::U32(x) => i128::from(x),
        ScalarValue::U64(x) => i128::from(x),
        ScalarValue::U128(x) => x.cast_signed(),
        ScalarValue::F32(x) => return (i128::from(x as i32), kind),
        ScalarValue::F64(x) => return (x as i128, kind),
        ScalarValue::Bool(b) => i128::from(b),
        ScalarValue::Ptr(p) => i128::from(p),
    };
    (n, kind)
}

/// UTF-8 bytes for `[u8; N] as str` / literal casts that lower to rodata [`IrInst::MakeStr`].
pub(super) fn utf8_bytes_for_str_cast(ctx: &LowerCtx<'_>, expr: &Expr) -> Option<Vec<u8>> {
    match expr {
        Expr::Literal(Literal::ByteString(b)) => {
            if std::str::from_utf8(b).is_ok() {
                Some(b.clone())
            } else {
                None
            }
        }
        Expr::Literal(Literal::String(s)) => Some(s.clone().into_bytes()),
        Expr::Ident(ident) => ctx.layout.binding(ident.symbol).and_then(|b| {
            if b.kind == BindingKind::Const {
                b.utf8_rodata.clone()
            } else {
                None
            }
        }),
        _ => None,
    }
}

fn literal_prim_kind(ctx: &LowerCtx<'_>, lit: &Literal, ty: TypeId) -> u8 {
    if let Some(k) = primitive_kind_for_type(&ctx.typed.types, ty) {
        return k.as_u8();
    }
    match lit {
        Literal::Int(i) => {
            if i.suffix == IntegerSuffix::Unsigned {
                PrimitiveKind::U32.as_u8()
            } else {
                PrimitiveKind::S32.as_u8()
            }
        }
        Literal::Float(_) => PrimitiveKind::F64.as_u8(),
        Literal::Bool(_) => PrimitiveKind::Bool.as_u8(),
        Literal::ByteChar(_) => PrimitiveKind::U8.as_u8(),
        _ => SLOT_KIND_AGG,
    }
}

pub(super) fn lower_literal(ctx: &mut LowerCtx<'_>, lit: &Literal, ty: TypeId) {
    if let Literal::String(s) = lit {
        let idx = ctx.intern_const(IrConst::Bytes(s.clone().into_bytes()));
        ctx.emit(IrInst::MakeStr { pool_index: idx });
        return;
    }
    if let Literal::ByteString(b) = lit {
        let elem_ty = match ctx.typed.types.get(ty) {
            Ty::Array { elem, .. } => *elem,
            _ => ty,
        };
        for &byte in b {
            let idx = ctx.intern_const(IrConst::Int(i128::from(byte), PrimitiveKind::U8));
            ctx.emit(IrInst::Const {
                index: idx,
                ty: elem_ty,
                prim_kind: PrimitiveKind::U8.as_u8(),
            });
        }
        let len = u32::try_from(b.len()).unwrap_or(0);
        ctx.emit(IrInst::MakeArray { len });
        return;
    }
    let index = intern_literal(ctx, lit, ty);
    ctx.emit(IrInst::Const {
        index,
        ty,
        prim_kind: literal_prim_kind(ctx, lit, ty),
    });
    if matches!(lit, Literal::Float(_))
        && matches!(
            ctx.typed.types.get(ty),
            Ty::Primitive(phx_syntax::token::Keyword::F32)
        )
    {
        ctx.emit(IrInst::Cast {
            from_kind: PrimitiveKind::F64.as_u8(),
            to_kind: PrimitiveKind::F32.as_u8(),
        });
    }
}

pub(super) fn lower_ident(ctx: &mut LowerCtx<'_>, ident: Ident, ty: TypeId) {
    let symbol = ident.symbol;
    if let Some(binding) = ctx.layout.binding(symbol) {
        ctx.emit(IrInst::LoadLocal {
            slot: binding.slot,
            ty,
            prim_kind: prim_kind_byte(ctx.typed, binding.ty),
        });
        return;
    }
    if let Some(def) = lookup_resolution(&ctx.typed.resolved, ctx.module, ident.id) {
        if let Some((target_kind, target_id)) = fn_ptr_target(ctx.typed, def) {
            ctx.emit(IrInst::MakeFnPtr {
                target_kind,
                target_id,
                ty,
            });
        }
    }
}

pub(super) fn lower_path(ctx: &mut LowerCtx<'_>, path: &Path, ty: TypeId) {
    if path.segments.len() == 1
        && let PathSegment::Ident(ident) = path.segments[0]
    {
        lower_ident(ctx, ident, ty);
    }
}

pub(super) fn lower_binary(
    ctx: &mut LowerCtx<'_>,
    op: BinOp,
    left: &ExprNode,
    right: &ExprNode,
    result_ty: TypeId,
) {
    match op {
        BinOp::Gt => {
            lower_expr(ctx, right);
            lower_expr(ctx, left);
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Lt,
                result: result_ty,
                prim_kind: prim_kind_byte(ctx.typed, result_ty),
            });
        }
        BinOp::Ge => {
            lower_expr(ctx, left);
            lower_expr(ctx, right);
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Ge,
                result: result_ty,
                prim_kind: prim_kind_byte(ctx.typed, result_ty),
            });
        }
        BinOp::Le => {
            lower_expr(ctx, left);
            lower_expr(ctx, right);
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Le,
                result: result_ty,
                prim_kind: prim_kind_byte(ctx.typed, result_ty),
            });
        }
        BinOp::Ne => {
            lower_expr(ctx, left);
            lower_expr(ctx, right);
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Ne,
                result: result_ty,
                prim_kind: prim_kind_byte(ctx.typed, result_ty),
            });
        }
        BinOp::Or | BinOp::And => {
            lower_short_circuit_bool(ctx, op, left, right, result_ty);
        }
        BinOp::Eq | BinOp::Lt | BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
            lower_expr(ctx, left);
            lower_expr(ctx, right);
            if let Some(ir_op) = binop_to_ir(op) {
                ctx.emit(IrInst::BinOp {
                    op: ir_op,
                    result: result_ty,
                    prim_kind: prim_kind_byte(ctx.typed, result_ty),
                });
            }
        }
        BinOp::BitOr
        | BinOp::BitXor
        | BinOp::BitAnd
        | BinOp::Shl
        | BinOp::Shr
        | BinOp::Mod
        | BinOp::Pow => {
            lower_expr(ctx, left);
            lower_expr(ctx, right);
            if let Some(ir_op) = binop_to_ir(op) {
                ctx.emit(IrInst::BinOp {
                    op: ir_op,
                    result: result_ty,
                    prim_kind: prim_kind_byte(ctx.typed, result_ty),
                });
            }
        }
    }
}

fn lower_short_circuit_bool(
    ctx: &mut LowerCtx<'_>,
    op: BinOp,
    left: &ExprNode,
    right: &ExprNode,
    _result_ty: TypeId,
) {
    lower_expr(ctx, left);

    let rhs_id = ctx.fresh_block();
    let short_id = ctx.fresh_block();
    let merge_id = ctx.fresh_block();

    match op {
        BinOp::And => {
            ctx.emit(IrInst::JumpIf {
                then_block: rhs_id,
                else_block: short_id,
            });
        }
        BinOp::Or => {
            ctx.emit(IrInst::JumpIf {
                then_block: short_id,
                else_block: rhs_id,
            });
        }
        _ => return,
    }

    ctx.set_current(short_id);
    let bool_ty_id = bool_ty(ctx.typed);
    let short_val = if op == BinOp::And {
        ctx.intern_const(IrConst::Bool(false))
    } else {
        ctx.intern_const(IrConst::Bool(true))
    };
    ctx.emit(IrInst::Const {
        index: short_val,
        ty: bool_ty_id,
        prim_kind: prim_kind_byte(ctx.typed, bool_ty_id),
    });
    ctx.emit(IrInst::Jump { target: merge_id });

    ctx.set_current(rhs_id);
    lower_expr(ctx, right);
    ctx.emit(IrInst::Jump { target: merge_id });

    ctx.set_current(merge_id);
}

fn binop_to_ir(op: BinOp) -> Option<IrBinOp> {
    match op {
        BinOp::Add => Some(IrBinOp::Add),
        BinOp::Sub => Some(IrBinOp::Sub),
        BinOp::Mul => Some(IrBinOp::Mul),
        BinOp::Div => Some(IrBinOp::Div),
        BinOp::Eq => Some(IrBinOp::Eq),
        BinOp::Lt => Some(IrBinOp::Lt),
        BinOp::Ne => Some(IrBinOp::Ne),
        BinOp::Le => Some(IrBinOp::Le),
        BinOp::Ge => Some(IrBinOp::Ge),
        BinOp::Mod => Some(IrBinOp::Mod),
        BinOp::Pow => Some(IrBinOp::Pow),
        BinOp::BitOr => Some(IrBinOp::BitOr),
        BinOp::BitXor => Some(IrBinOp::BitXor),
        BinOp::BitAnd => Some(IrBinOp::BitAnd),
        BinOp::Shl => Some(IrBinOp::Shl),
        BinOp::Shr => Some(IrBinOp::Shr),
        BinOp::Or | BinOp::And | BinOp::Gt => None,
    }
}

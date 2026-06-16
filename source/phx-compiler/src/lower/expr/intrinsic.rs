//! VM intrinsic call lowering.

use crate::ir::IrConst;
use crate::ir::IrInst;
use crate::lower::ctx::LowerCtx;
use crate::typeck::{ExprId, IntrinsicSite, Ty, TypeId, primitive_kind_for_type};

/// Lowers a single VM intrinsic call site.
pub(super) fn lower_intrinsic_call(
    ctx: &mut LowerCtx<'_>,
    site: IntrinsicSite,
    result_ty: TypeId,
    expr_id: ExprId,
) {
    match site {
        IntrinsicSite::AllocBytes => {
            ctx.emit_here(IrInst::Alloc { result: result_ty });
        }
        IntrinsicSite::DeallocBytes => {
            ctx.emit_here(IrInst::Free);
        }
        IntrinsicSite::SliceFromRawParts => {
            let elem_kind = match ctx.typed.types.get(result_ty) {
                Ty::Slice(elem) => primitive_kind_for_type(&ctx.typed.types, *elem).map_or(
                    phx_bytecode::SLOT_KIND_AGG,
                    phx_bytecode::PrimitiveKind::as_u8,
                ),
                _ => phx_bytecode::SLOT_KIND_AGG,
            };
            ctx.emit_here(IrInst::MakeSliceFromPtr { elem_kind });
        }
        IntrinsicSite::SliceLen => {
            ctx.emit_here(IrInst::SliceLen);
        }
        IntrinsicSite::SizeOf => {
            let bytes = ctx
                .typed
                .size_of_literals
                .get(&expr_id)
                .copied()
                .or_else(|| size_of_bytes_for_specialized_fn(ctx))
                .unwrap_or(0);
            let idx = ctx.intern_const(IrConst::Int(
                i128::from(bytes),
                phx_bytecode::PrimitiveKind::U32,
            ));
            ctx.emit_here(IrInst::Const {
                index: idx,
                ty: result_ty,
                prim_kind: phx_bytecode::PrimitiveKind::U32.as_u8(),
            });
        }
    }
}

fn size_of_bytes_for_specialized_fn(ctx: &LowerCtx<'_>) -> Option<u32> {
    let base = ctx.typed.specialized_from.get(&ctx.layout.def)?;
    let inst = ctx.typed.mono_insts.iter().find(|i| i.base_fn == *base)?;
    let elem = *inst.args.first()?;
    crate::typeck::type_byte_size(
        &ctx.typed.types,
        &ctx.typed.layout,
        &ctx.typed.resolved,
        elem,
    )
}

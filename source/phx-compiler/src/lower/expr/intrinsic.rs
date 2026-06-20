//! VM intrinsic call lowering.
//!
//! **Pipeline position:** [`super::call`] postfix and static-call paths, during expression lowering
//! after typeck. Does not resolve names or type-check — consumes [`IntrinsicSite`](crate::typeck::IntrinsicSite)
//! markers typeck recorded on intrinsic call sites.
//!
//! **Inputs:** mutable [`LowerCtx`], the intrinsic variant, typeck `result_ty`, and the call site's
//! [`ExprId`](crate::typeck::ExprId) (for compile-time `size_of` folds).
//!
//! **Outputs:** one or more [`IrInst`](crate::ir::IrInst) records appended to the current basic
//! block via [`LowerCtx::emit_here`](crate::lower::ctx::LowerCtx::emit_here); expression values
//! remain on the implicit stack per each instruction's stack effect.
//!
//! Maps each [`IntrinsicSite`] to VM-facing IR: allocation, slice construction, length queries,
//! and folded `size_of` constants. Invoked when call resolution finds a compiler-known intrinsic
//! rather than a user [`FunctionLayout`](crate::typeck::FunctionLayout) body.
//!
//! [`IntrinsicSite::SizeOf`] reads the compile-time byte size from
//! [`TypedProgram::size_of_literals`](crate::typeck::TypedProgram::size_of_literals), with a
//! fallback for specialized generic functions via [`size_of_bytes_for_specialized_fn`].

use crate::ir::IrConst;
use crate::ir::IrInst;
use crate::lower::ctx::LowerCtx;
use crate::typeck::{ExprId, IntrinsicSite, Ty, TypeId, primitive_kind_for_type};

/// Lowers one compiler-known VM intrinsic call site to IR.
///
/// Dispatches on `site` to emit a single stack-effect instruction (or a folded constant for
/// `SizeOf`). `result_ty` tags aggregate vs primitive results on instructions that carry a type
/// id; `expr_id` keys [`TypedProgram::size_of_literals`](crate::typeck::TypedProgram::size_of_literals)
/// for the `SizeOf` compile-time fold.
///
/// Invoked from [`super::call`] when postfix or static call resolution finds an intrinsic marker
/// instead of a user function layout.
///
/// # Panics
///
/// Never panics on malformed user input. `SizeOf` without a recorded literal or specialized-fn
/// fallback emits a zero-byte `u32` constant (degenerate but safe).
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

/// Byte size for `size_of` inside a monomorphized specialized function when the literal map missed.
///
/// Follows [`TypedProgram::specialized_from`](crate::typeck::TypedProgram::specialized_from) back
/// to the template function and uses the first mono instantiation arg with
/// [`type_byte_size`](crate::typeck::type_byte_size).
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

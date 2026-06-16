//! Postfix chains, calls, methods, and `?` lowering.

use phx_syntax::Symbol;
use phx_syntax::ast::expr::{Expr, ExprNode, PostfixOp};
use phx_syntax::ast::ident::PathSegment;

use crate::ir::{IrBinOp, IrInst};
use crate::lower::ctx::{LowerCtx, lookup_resolution, prim_kind_byte, slot_for_symbol};
use crate::resolver::{DefId, DefKind};
use crate::typeck::{
    BindingKind, ExprId, FunctionLayout, LocalSlot, PrimitiveMethodSite, TryFailureMode,
    TrySiteMeta, Ty, TypeId, TypedProgram,
};
use phx_diagnostics::LowerError;

use super::intrinsic::lower_intrinsic_call;
use super::literal::lower_ident;
use super::{lower_expr, lower_expr_inner, lower_expr_typed, named_type_parts};

pub(super) fn lower_postfix(
    ctx: &mut LowerCtx<'_>,
    base: &ExprNode,
    ops: &[PostfixOp],
    result_ty: TypeId,
    postfix_expr_id: ExprId,
) {
    lower_postfix_inner(ctx, base, ops, result_ty, postfix_expr_id);
}

fn function_layout(typed: &TypedProgram, def: DefId) -> Option<&FunctionLayout> {
    typed.functions.iter().find(|f| f.def == def)
}

fn method_callee_from_site(typed: &TypedProgram, site: ExprId) -> Option<DefId> {
    typed.method_call_sites.get(&site).map(|meta| meta.template)
}

fn method_ref_receiver_ty(ctx: &LowerCtx<'_>, callee: DefId) -> Option<TypeId> {
    let layout = function_layout(ctx.typed, callee)?;
    let first_param = layout
        .bindings
        .iter()
        .find(|b| b.kind == BindingKind::Param)?;
    match ctx.typed.types.get(first_param.ty) {
        Ty::Ref { .. } => Some(first_param.ty),
        _ => None,
    }
}

fn single_method_with_ref_receiver(
    ctx: &LowerCtx<'_>,
    ops: &[PostfixOp],
    postfix_expr_id: ExprId,
) -> Option<(DefId, TypeId)> {
    if ops.len() != 1 {
        return None;
    }
    let PostfixOp::Method { .. } = &ops[0] else {
        return None;
    };
    let callee = method_callee_from_site(ctx.typed, postfix_expr_id)?;
    let ref_ty = method_ref_receiver_ty(ctx, callee)?;
    Some((callee, ref_ty))
}

fn emit_ref_method_receiver(ctx: &mut LowerCtx<'_>, base: &ExprNode) {
    if let Expr::Ident(ident) = &base.inner
        && let Some(slot) = slot_for_symbol(ctx.layout, ident.symbol)
    {
        ctx.emit_here(IrInst::AddressOfLocal { slot });
    }
}

/// Stores the evaluated receiver value and passes `&mut` / `&` for method calls after field chains.
fn emit_ref_receiver_from_stack_value(ctx: &mut LowerCtx<'_>, callee: DefId, value_ty: TypeId) {
    if method_ref_receiver_ty(ctx, callee).is_none() {
        return;
    }
    let slot = ctx.next_match_temp();
    ctx.emit_here(IrInst::StoreLocal {
        slot,
        ty: value_ty,
        prim_kind: prim_kind_byte(ctx.typed, value_ty),
    });
    ctx.emit_here(IrInst::AddressOfLocal { slot });
}

#[allow(clippy::too_many_lines)]
fn lower_postfix_inner(
    ctx: &mut LowerCtx<'_>,
    base: &ExprNode,
    ops: &[PostfixOp],
    result_ty: TypeId,
    postfix_expr_id: ExprId,
) {
    let static_call = !ops.is_empty()
        && matches!(ops[0], PostfixOp::Call { .. })
        && resolve_call_callee(ctx, base).is_some();
    if static_call {
        if let PostfixOp::Call { args, .. } = &ops[0] {
            // Typeck assigns an expr id to the call base; static `Call` does not load it.
            let _ = ctx.expr_ty();
            let Some(callee) = resolve_call_callee(ctx, base) else {
                ctx.error_unresolved_callee(base.span);
                return;
            };
            for arg in args {
                lower_expr(ctx, arg);
            }
            let call_result_ty = if ops.len() == 1 {
                result_ty
            } else {
                let callee_ty = ctx
                    .typed
                    .value_types
                    .get(&callee)
                    .copied()
                    .unwrap_or(result_ty);
                match ctx.typed.types.get(callee_ty) {
                    Ty::Fn { ret, .. } => *ret,
                    _ => result_ty,
                }
            };
            emit_call_or_intrinsic(ctx, callee, call_result_ty, postfix_expr_id);
            if ops.len() > 1 {
                let mut receiver_ty = call_result_ty;
                for op in &ops[1..] {
                    match op {
                        PostfixOp::Field(field) => {
                            if matches!(ctx.typed.types.get(receiver_ty), Ty::Ref { .. }) {
                                ctx.emit_here(IrInst::LoadAggViaLocalPtr);
                                receiver_ty = match ctx.typed.types.get(receiver_ty) {
                                    Ty::Ref { inner, .. } => *inner,
                                    _ => receiver_ty,
                                };
                            }
                            if let Some((def, args)) = named_type_parts(ctx.typed, receiver_ty) {
                                let type_id =
                                    ctx.typed.layout.type_id_for_named(def, &args).unwrap_or(0);
                                let field_index = ctx
                                    .typed
                                    .layout
                                    .struct_field_index(def, field.symbol, &args)
                                    .unwrap_or(0);
                                let field_ty = ctx
                                    .typed
                                    .layout
                                    .struct_layout(def, &args)
                                    .and_then(|sl| sl.fields.get(field_index as usize))
                                    .map_or(receiver_ty, |(_, ty)| *ty);
                                ctx.emit_here(IrInst::GetField {
                                    type_id,
                                    field_index,
                                    result: field_ty,
                                });
                                receiver_ty = field_ty;
                            }
                        }
                        PostfixOp::Method { args, .. } => {
                            for arg in args {
                                lower_expr(ctx, arg);
                            }
                            if let Some(callee) =
                                method_callee_from_site(ctx.typed, postfix_expr_id)
                            {
                                emit_ref_receiver_from_stack_value(ctx, callee, receiver_ty);
                                emit_call_or_intrinsic(ctx, callee, result_ty, postfix_expr_id);
                            } else if let Some(meta) =
                                ctx.typed.indirect_call_sites.get(&postfix_expr_id)
                            {
                                ctx.emit_here(IrInst::CallIndirect {
                                    sig_type_id: meta.sig_type_id,
                                    expected_arity: meta.expected_arity,
                                    ret: result_ty,
                                    foreign: meta.foreign,
                                });
                            }
                            receiver_ty = result_ty;
                        }
                        PostfixOp::Call { args, .. } => {
                            for arg in args {
                                lower_expr(ctx, arg);
                            }
                            if let Some(callee) =
                                method_callee_from_site(ctx.typed, postfix_expr_id)
                            {
                                emit_ref_receiver_from_stack_value(ctx, callee, receiver_ty);
                                emit_call_or_intrinsic(ctx, callee, result_ty, postfix_expr_id);
                            } else if let Some(meta) =
                                ctx.typed.indirect_call_sites.get(&postfix_expr_id)
                            {
                                ctx.emit_here(IrInst::CallIndirect {
                                    sig_type_id: meta.sig_type_id,
                                    expected_arity: meta.expected_arity,
                                    ret: result_ty,
                                    foreign: meta.foreign,
                                });
                            }
                            receiver_ty = result_ty;
                        }
                        PostfixOp::Index(idx) => {
                            lower_expr(ctx, idx);
                            ctx.emit_here(IrInst::Index { result: result_ty });
                            receiver_ty = result_ty;
                        }
                        PostfixOp::Try => {
                            if let Some(meta) = ctx.typed.try_sites.get(&postfix_expr_id) {
                                lower_try(ctx, meta);
                                receiver_ty = result_ty;
                            }
                        }
                    }
                }
            }
        }
        return;
    }

    let ref_method = single_method_with_ref_receiver(ctx, ops, postfix_expr_id);
    let mut receiver_ty = if let Some((_, ref_ty)) = ref_method {
        let _ = ctx.expr_ty();
        emit_ref_method_receiver(ctx, base);
        ref_ty
    } else if let Expr::Ident(ident) = &base.inner
        && let Some(binding) = ctx.layout.binding(ident.symbol)
    {
        // Generic impl templates may omit body typeck (`expr_types` empty); use the layout slot type.
        let _ = ctx.expr_ty();
        lower_ident(ctx, *ident, binding.ty);
        binding.ty
    } else {
        lower_expr_typed(ctx, base)
    };
    for op in ops {
        match op {
            PostfixOp::Field(field) => {
                if matches!(ctx.typed.types.get(receiver_ty), Ty::Ref { .. }) {
                    ctx.emit_here(IrInst::LoadAggViaLocalPtr);
                    receiver_ty = match ctx.typed.types.get(receiver_ty) {
                        Ty::Ref { inner, .. } => *inner,
                        _ => receiver_ty,
                    };
                }
                if let Some((def, args)) = named_type_parts(ctx.typed, receiver_ty) {
                    let type_id = ctx.typed.layout.type_id_for_named(def, &args).unwrap_or(0);
                    let field_index = ctx
                        .typed
                        .layout
                        .struct_field_index(def, field.symbol, &args)
                        .unwrap_or(0);
                    let field_ty = ctx
                        .typed
                        .layout
                        .struct_layout(def, &args)
                        .and_then(|s| s.fields.get(field_index as usize))
                        .map_or(result_ty, |(_, ty)| *ty);
                    ctx.emit_here(IrInst::GetField {
                        type_id,
                        field_index,
                        result: field_ty,
                    });
                    receiver_ty = field_ty;
                }
            }
            PostfixOp::Method { name, args, .. } => {
                if let Some(site) = ctx.typed.primitive_method_sites.get(&postfix_expr_id) {
                    lower_primitive_method(ctx, *site, receiver_ty, args, result_ty);
                    receiver_ty = result_ty;
                    continue;
                }
                let callee = ref_method
                    .as_ref()
                    .map(|(c, _)| *c)
                    .or_else(|| method_callee_from_site(ctx.typed, postfix_expr_id));
                if let Some(callee) = callee {
                    if ref_method.is_none() {
                        emit_ref_receiver_from_stack_value(ctx, callee, receiver_ty);
                    }
                    for arg in args {
                        lower_expr(ctx, arg);
                    }
                    emit_call_or_intrinsic(ctx, callee, result_ty, postfix_expr_id);
                } else {
                    for arg in args {
                        lower_expr(ctx, arg);
                    }
                    ctx.error_unresolved_callee(name.span);
                }
            }
            PostfixOp::Call { args, .. } => {
                if let Some(callee) = ctx.typed.associated_fn_sites.get(&postfix_expr_id).copied() {
                    for arg in args {
                        lower_expr(ctx, arg);
                    }
                    emit_call_or_intrinsic(ctx, callee, result_ty, postfix_expr_id);
                    receiver_ty = result_ty;
                } else if let Some(struct_def) = resolve_tuple_struct_ctor(ctx, base) {
                    for arg in args {
                        lower_expr(ctx, arg);
                    }
                    let (type_id, field_count) =
                        tuple_struct_make_operands(ctx, struct_def, result_ty);
                    ctx.emit_here(IrInst::MakeStruct {
                        type_id,
                        field_count,
                    });
                    receiver_ty = result_ty;
                } else if let Some(variant_def) = resolve_variant_ctor(ctx, base) {
                    if let Some(meta) = ctx.typed.layout.variants.get(&variant_def) {
                        for arg in args {
                            lower_expr(ctx, arg);
                        }
                        let enum_args = match ctx.typed.types.get(result_ty) {
                            Ty::Named { def, args } if *def == meta.enum_def => args.clone(),
                            _ => Vec::new(),
                        };
                        let type_id = ctx
                            .typed
                            .layout
                            .type_id_for_named(meta.enum_def, &enum_args)
                            .unwrap_or(0);
                        let payload_count = u32::try_from(args.len()).unwrap_or(u32::MAX);
                        ctx.emit_here(IrInst::MakeEnum {
                            type_id,
                            variant_tag: meta.tag,
                            payload_count,
                        });
                        receiver_ty = result_ty;
                    }
                } else if let Some(site) = ctx.intrinsic_site_for_expr(postfix_expr_id) {
                    for arg in args {
                        lower_expr(ctx, arg);
                    }
                    lower_intrinsic_call(ctx, site, result_ty, postfix_expr_id);
                    receiver_ty = result_ty;
                } else if let Some(meta) = ctx.typed.indirect_call_sites.get(&postfix_expr_id) {
                    for arg in args {
                        lower_expr(ctx, arg);
                    }
                    ctx.emit_here(IrInst::CallIndirect {
                        sig_type_id: meta.sig_type_id,
                        expected_arity: meta.expected_arity,
                        ret: result_ty,
                        foreign: meta.foreign,
                    });
                    receiver_ty = result_ty;
                } else {
                    let Some(callee) = resolve_call_callee(ctx, base) else {
                        ctx.error_unresolved_callee(base.span);
                        continue;
                    };
                    for arg in args {
                        lower_expr(ctx, arg);
                    }
                    emit_call_or_intrinsic(ctx, callee, result_ty, postfix_expr_id);
                }
            }
            PostfixOp::Index(idx) => {
                lower_expr(ctx, idx);
                ctx.emit_here(IrInst::Index { result: result_ty });
            }
            PostfixOp::Try => {
                if let Some(meta) = ctx.typed.try_sites.get(&postfix_expr_id) {
                    lower_try(ctx, meta);
                    receiver_ty = result_ty;
                }
            }
        }
    }
}

/// Lowers `expr?` using `MatchTag` and early `Return` on the failure variant.
fn lower_try(ctx: &mut LowerCtx<'_>, meta: &TrySiteMeta) {
    let Some(bytecode_type_id) = ctx
        .typed
        .layout
        .type_id_for_named(meta.enum_def, &meta.enum_args)
    else {
        debug_assert!(
            false,
            "missing bytecode type id for `?` scrutinee enum (mono layout)"
        );
        return;
    };
    let temp = meta.temp_slot;
    let temp_ty = meta.scrutinee_ty;
    let prim = prim_kind_byte(ctx.typed, temp_ty);
    ctx.emit_here(IrInst::StoreLocal {
        slot: temp,
        ty: temp_ty,
        prim_kind: prim,
    });

    let succ_block = ctx.fresh_block();
    let fail_block = ctx.fresh_block();
    let cont_block = ctx.fresh_block();

    ctx.emit_here(IrInst::LoadLocal {
        slot: temp,
        ty: temp_ty,
        prim_kind: prim,
    });
    ctx.emit_here(IrInst::MatchTag {
        type_id: bytecode_type_id,
        variant_tag: meta.success_tag,
    });
    ctx.emit_here(IrInst::JumpIf {
        then_block: succ_block,
        else_block: fail_block,
    });

    ctx.set_current(succ_block);
    ctx.emit_here(IrInst::LoadLocal {
        slot: temp,
        ty: temp_ty,
        prim_kind: prim,
    });
    ctx.emit_here(IrInst::GetField {
        type_id: bytecode_type_id,
        field_index: 0,
        result: meta.success_ty,
    });
    ctx.emit_here(IrInst::Jump { target: cont_block });

    ctx.set_current(fail_block);
    match &meta.failure_mode {
        TryFailureMode::ReturnScrutinee => {
            lower_try_return_scrutinee(ctx, temp, temp_ty, prim);
        }
        TryFailureMode::ConvertErr { .. } => {
            lower_try_convert_err(ctx, meta, bytecode_type_id, temp, temp_ty, prim);
        }
    }

    ctx.set_current(cont_block);
}

fn lower_try_return_scrutinee(ctx: &mut LowerCtx<'_>, temp: LocalSlot, temp_ty: TypeId, prim: u8) {
    ctx.emit_here(IrInst::LoadLocal {
        slot: temp,
        ty: temp_ty,
        prim_kind: prim,
    });
    ctx.emit_here(IrInst::Return {
        ty: ctx.layout.return_type,
    });
}

/// Lowers the failure arm of `?` when `From` converts the error payload.
///
/// # Errors
///
/// Records [`LowerError::MissingTryConvertLayout`] when typeck layout metadata for the
/// enclosing return `Result` is missing (internal invariant violation).
fn lower_try_convert_err(
    ctx: &mut LowerCtx<'_>,
    meta: &TrySiteMeta,
    bytecode_type_id: u32,
    temp: LocalSlot,
    temp_ty: TypeId,
    prim: u8,
) {
    let TryFailureMode::ConvertErr {
        from_fn,
        err_in_ty,
        err_out_ty,
        return_result_ty,
    } = &meta.failure_mode
    else {
        return;
    };
    let callee = ctx
        .typed
        .specialized_from
        .get(from_fn)
        .copied()
        .unwrap_or(*from_fn);
    ctx.emit_here(IrInst::LoadLocal {
        slot: temp,
        ty: temp_ty,
        prim_kind: prim,
    });
    ctx.emit_here(IrInst::GetField {
        type_id: bytecode_type_id,
        field_index: 0,
        result: *err_in_ty,
    });
    ctx.emit_here(IrInst::Call {
        callee,
        ret: *err_out_ty,
    });
    let Ty::Named {
        def: ret_enum_def,
        args: ret_enum_args,
    } = ctx.typed.types.get(*return_result_ty).clone()
    else {
        ctx.bag.push(
            ctx.module,
            LowerError::MissingTryConvertLayout {
                detail: "return Result type",
            },
        );
        return;
    };
    let Some(ret_bytecode_type_id) = ctx
        .typed
        .layout
        .type_id_for_named(ret_enum_def, &ret_enum_args)
    else {
        ctx.bag.push(
            ctx.module,
            LowerError::MissingTryConvertLayout {
                detail: "bytecode type id for return Result",
            },
        );
        return;
    };
    let Some(err_tag) = ctx.typed.lang_items.failure_tag_for(
        &ctx.typed.layout,
        &ctx.typed.types,
        *return_result_ty,
    ) else {
        ctx.bag.push(
            ctx.module,
            LowerError::MissingTryConvertLayout {
                detail: "Err tag for return Result",
            },
        );
        return;
    };
    ctx.emit_here(IrInst::MakeEnum {
        type_id: ret_bytecode_type_id,
        variant_tag: err_tag,
        payload_count: 1,
    });
    ctx.emit_here(IrInst::Return {
        ty: ctx.layout.return_type,
    });
}

fn emit_call_or_intrinsic(
    ctx: &mut LowerCtx<'_>,
    callee: DefId,
    result_ty: TypeId,
    expr_id: ExprId,
) {
    if let Some(site) = ctx.intrinsic_site_for_expr(expr_id) {
        lower_intrinsic_call(ctx, site, result_ty, expr_id);
        return;
    }
    if let Some(site) = ctx.typed.lang_items.site_for_call(callee) {
        lower_intrinsic_call(ctx, site, result_ty, expr_id);
        return;
    }
    ctx.emit_here(IrInst::Call {
        callee,
        ret: result_ty,
    });
    if callee_returns_unit(ctx, callee, result_ty) {
        ctx.emit_here(IrInst::Pop);
    }
}

fn callee_returns_unit(ctx: &LowerCtx<'_>, callee: DefId, fallback: TypeId) -> bool {
    if let Some(layout) = ctx.typed.functions.iter().find(|f| f.def == callee) {
        if matches!(ctx.typed.types.get(layout.return_type), Ty::Unit) {
            return true;
        }
    }
    if let Some(&fn_ty) = ctx.typed.value_types.get(&callee) {
        if let Ty::Fn { ret, .. } = ctx.typed.types.get(fn_ty).clone() {
            return matches!(ctx.typed.types.get(ret), Ty::Unit);
        }
    }
    matches!(ctx.typed.types.get(fallback), Ty::Unit)
}

fn lower_primitive_method(
    ctx: &mut LowerCtx<'_>,
    site: PrimitiveMethodSite,
    receiver_ty: TypeId,
    args: &[ExprNode],
    result_ty: TypeId,
) {
    let prim_kind = prim_kind_byte(ctx.typed, receiver_ty);
    match site {
        PrimitiveMethodSite::Eq => {
            for arg in args {
                lower_expr(ctx, arg);
            }
            ctx.emit_here(IrInst::BinOp {
                op: IrBinOp::Eq,
                result: result_ty,
                prim_kind,
            });
        }
        PrimitiveMethodSite::Clone => {
            // Receiver value already on stack; clone is identity for Copyable primitives.
            let _ = result_ty;
        }
    }
}

fn tuple_struct_make_operands(
    ctx: &LowerCtx<'_>,
    struct_def: DefId,
    result_ty: TypeId,
) -> (u32, u32) {
    let (def, args) = named_type_parts(ctx.typed, result_ty).unwrap_or((struct_def, Vec::new()));
    let type_id = ctx.typed.layout.type_id_for_named(def, &args).unwrap_or(0);
    let field_count = ctx
        .typed
        .layout
        .struct_layout(def, &args)
        .map_or(0, |sl| u32::try_from(sl.fields.len()).unwrap_or(u32::MAX));
    (type_id, field_count)
}

fn resolve_tuple_struct_ctor(ctx: &LowerCtx<'_>, base: &ExprNode) -> Option<DefId> {
    let node_id = path_or_ident_node_id(&base.inner)?;
    let def = lookup_resolution(&ctx.typed.resolved, ctx.module, node_id)?;
    if ctx.typed.layout.tuple_structs.contains(&def) {
        Some(def)
    } else {
        None
    }
}

fn resolve_variant_ctor(ctx: &LowerCtx<'_>, base: &ExprNode) -> Option<DefId> {
    let node_id = path_or_ident_node_id(&base.inner)?;
    let def = lookup_resolution(&ctx.typed.resolved, ctx.module, node_id)?;
    if ctx.typed.layout.variants.contains_key(&def) {
        Some(def)
    } else {
        None
    }
}

fn resolve_call_callee(ctx: &LowerCtx<'_>, base: &ExprNode) -> Option<DefId> {
    let node_id = path_or_ident_node_id(&base.inner)?;
    let def = lookup_resolution(&ctx.typed.resolved, ctx.module, node_id)?;
    if ctx
        .typed
        .resolved
        .defs
        .get(def.index() as usize)
        .is_some_and(|d| d.kind == DefKind::Fn)
    {
        Some(def)
    } else {
        None
    }
}

pub(super) fn path_or_ident_node_id(expr: &Expr) -> Option<phx_syntax::AstNodeId> {
    match expr {
        Expr::Ident(ident) => Some(ident.id),
        Expr::Path(path) if path.segments.len() == 1 => match &path.segments[0] {
            PathSegment::Ident(ident) => Some(ident.id),
            PathSegment::Type(seg) => Some(seg.name.id),
        },
        _ => None,
    }
}

pub(super) fn struct_def_from_ty(ctx: &LowerCtx<'_>, ty: TypeId) -> Option<DefId> {
    match ctx.typed.types.get(ty) {
        Ty::Named { def, .. } if ctx.typed.layout.structs.contains_key(def) => Some(*def),
        Ty::Ref { inner, .. } => struct_def_from_ty(ctx, *inner),
        _ => None,
    }
}

pub(super) fn struct_args_from_base(ctx: &LowerCtx<'_>, base: &ExprNode) -> Vec<TypeId> {
    if let Expr::Ident(ident) = &base.inner {
        if let Some(binding) = ctx.layout.binding(ident.symbol) {
            let inner = match ctx.typed.types.get(binding.ty) {
                Ty::Ref { inner, .. } => *inner,
                _ => binding.ty,
            };
            if let Ty::Named { args, .. } = ctx.typed.types.get(inner) {
                return args.clone();
            }
        }
    }
    Vec::new()
}

pub(super) fn struct_def_from_base(ctx: &LowerCtx<'_>, base: &ExprNode) -> Option<DefId> {
    if let Expr::Ident(ident) = &base.inner {
        if let Some(binding) = ctx.layout.binding(ident.symbol) {
            return struct_def_from_ty(ctx, binding.ty);
        }
    }
    None
}

pub(super) fn binding_is_ref_to_struct(ctx: &LowerCtx<'_>, symbol: Symbol) -> bool {
    ctx.layout
        .binding(symbol)
        .is_some_and(|b| matches!(ctx.typed.types.get(b.ty), Ty::Ref { inner, .. } if struct_def_from_ty(ctx, *inner).is_some()))
}

pub(super) fn emit_load_struct_base(ctx: &mut LowerCtx<'_>, base: &ExprNode) {
    let ty = ctx.expr_ty();
    let expr_id = ExprId::from_raw(ctx.next_expr - 1);
    if let Expr::Ident(ident) = &base.inner {
        if binding_is_ref_to_struct(ctx, ident.symbol) {
            lower_ident(ctx, *ident, ty);
            ctx.emit_here(IrInst::LoadAggViaLocalPtr);
            return;
        }
    }
    lower_expr_inner(ctx, &base.inner, ty, expr_id);
}

pub(super) fn store_base_local(ctx: &mut LowerCtx<'_>, base: &ExprNode) {
    if let Expr::Ident(ident) = &base.inner
        && let Some(binding) = ctx.layout.binding(ident.symbol)
        && !matches!(ctx.typed.types.get(binding.ty), Ty::Ref { .. })
    {
        ctx.emit_here(IrInst::StoreLocal {
            slot: binding.slot,
            ty: binding.ty,
            prim_kind: prim_kind_byte(ctx.typed, binding.ty),
        });
    }
}

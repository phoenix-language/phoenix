//! Lower expressions to IR instructions (stack-oriented).
//!
//! Each expression node visited by typeck [`check_expr_node`](crate::typeck::check::expr::TypeChecker::check_expr_node)
//! gets one [`ExprId`](crate::typeck::ExprId); lowering consumes them in the same pre-order via
//! [`LowerCtx::expr_ty`](crate::lower::ctx::LowerCtx::expr_ty). A few paths skip codegen but still
//! advance the cursor — keep these in sync with typeck:
//!
//! - static `f(args)` — base ident consumed in [`call`] without loading it
//! - `&ident` — operand ident consumed in this module after `AddressOfLocal`
//! - `arr as str` fast path — inner operand consumed after peek in the `Cast` arm
//! - ref-receiver / ident-receiver method calls — base consumed in [`call`]
//!
//! [`check_expr_node_infer`](crate::typeck::check::expr::TypeChecker::check_expr_node_infer) does
//! **not** allocate ids (generic inference only); lowering never sees those visits.
//!
//! ## Module map
//!
//! - [`literal`] — literals, identifiers, paths, binary operators
//! - [`assign`] — assignment and index/field stores
//! - [`call`] — postfix chains, calls, methods, `?`
//! - [`intrinsic`] — VM intrinsic lowering
//! - [`r#match`] — `if`, `match`, and block expressions

mod assign;
mod call;
mod intrinsic;
mod literal;
mod r#match;
mod prim;

use phx_syntax::Symbol;
use phx_syntax::ast::expr::{Expr, ExprNode, StructFieldInit};

use crate::ir::IrInst;
use crate::lower::ctx::{LowerCtx, prim_kind_byte, struct_def_by_name};
use crate::resolver::{DefId, DefKind};
use crate::typeck::TypedProgram;
use crate::typeck::{ExprId, Ty, TypeId, primitive_kind_for_type, primitive_load_signed};
use phx_bytecode::SLOT_KIND_AGG;

pub(crate) use assign::lower_assign_expr;
pub(crate) use r#match::{
    bind_match_pattern, block_ends_with_unconditional_jump, emit_arm_condition,
};

use call::lower_postfix;
use literal::{lower_binary, lower_ident, lower_literal, lower_path, utf8_bytes_for_str_cast};
use r#match::{lower_block_expr, lower_if, lower_match};

/// Lowers `expr` so its value is on the implicit stack.
pub fn lower_expr(ctx: &mut LowerCtx<'_>, expr: &ExprNode) {
    ctx.set_site(expr.span);
    let ty = ctx.expr_ty();
    let expr_id = ExprId::from_raw(ctx.next_expr - 1);
    lower_expr_inner(ctx, &expr.inner, ty, expr_id);
}

/// Lowers `expr` with an explicit type (generic impl templates without body `expr_types`).
pub fn lower_expr_with_type(ctx: &mut LowerCtx<'_>, expr: &ExprNode, ty: TypeId) {
    ctx.set_site(expr.span);
    let _ = ctx.expr_ty();
    let expr_id = ExprId::from_raw(ctx.next_expr - 1);
    lower_expr_inner(ctx, &expr.inner, ty, expr_id);
}

/// Lowers `expr` and returns its typeck-assigned type.
pub(super) fn lower_expr_typed(ctx: &mut LowerCtx<'_>, expr: &ExprNode) -> TypeId {
    ctx.set_site(expr.span);
    let ty = ctx.expr_ty();
    let expr_id = ExprId::from_raw(ctx.next_expr - 1);
    lower_expr_inner(ctx, &expr.inner, ty, expr_id);
    ty
}

#[allow(clippy::too_many_lines)]
pub(super) fn lower_expr_inner(
    ctx: &mut LowerCtx<'_>,
    expr: &Expr,
    result_ty: TypeId,
    expr_id: ExprId,
) {
    match expr {
        Expr::Literal(lit) => lower_literal(ctx, lit, result_ty),
        Expr::Ident(ident) => lower_ident(ctx, *ident, result_ty),
        Expr::Path(path) => lower_path(ctx, path, result_ty),
        Expr::Tuple(items) => {
            for item in items {
                lower_expr(ctx, item);
            }
            let arity = u32::try_from(items.len()).unwrap_or(u32::MAX);
            ctx.emit_here(IrInst::MakeTuple { arity });
        }
        Expr::Array(items) => {
            for item in items {
                lower_expr(ctx, item);
            }
            let len = u32::try_from(items.len()).unwrap_or(u32::MAX);
            ctx.emit_here(IrInst::MakeArray { len });
        }
        Expr::Unary { op, operand } => {
            use phx_syntax::ast::expr::UnaryOp;
            if matches!(op, UnaryOp::Ref | UnaryOp::RefMut) {
                if let Expr::Ident(ident) = &operand.inner {
                    if let Some(slot) = crate::lower::ctx::slot_for_symbol(ctx.layout, ident.symbol)
                    {
                        ctx.emit_here(IrInst::AddressOfLocal { slot });
                    }
                    // Operand ident is a separate typeck expression node; consume its cursor.
                    let _ = ctx.expr_ty();
                }
                return;
            }
            if matches!(op, UnaryOp::Deref) {
                let operand_id = ExprId::from_raw(ctx.next_expr);
                let operand_ty = ctx
                    .typed
                    .expr_types
                    .get(&operand_id)
                    .copied()
                    .unwrap_or(result_ty);
                if let (Ty::Ref { inner, .. }, Expr::Ident(ident)) =
                    (ctx.typed.types.get(operand_ty), &operand.inner)
                {
                    if let Some(kind) = primitive_kind_for_type(&ctx.typed.types, *inner) {
                        if let Some(binding) = ctx.layout.binding(ident.symbol) {
                            let _ = ctx.expr_ty();
                            ctx.emit_here(IrInst::LoadLocal {
                                slot: binding.slot,
                                ty: binding.ty,
                                prim_kind: prim_kind_byte(ctx.typed, binding.ty),
                            });
                            ctx.emit_here(IrInst::PtrLoad {
                                prim_kind: kind.as_u8(),
                                signed: primitive_load_signed(kind),
                                result: *inner,
                            });
                            return;
                        }
                    }
                }
            }
            let operand_id = ExprId::from_raw(ctx.next_expr);
            lower_expr(ctx, operand);
            match op {
                UnaryOp::Neg => {
                    ctx.emit_here(IrInst::Neg {
                        result: result_ty,
                        prim_kind: prim_kind_byte(ctx.typed, result_ty),
                    });
                }
                UnaryOp::Not => {
                    ctx.emit_here(IrInst::Not {
                        result: result_ty,
                        prim_kind: prim_kind_byte(ctx.typed, result_ty),
                    });
                }
                UnaryOp::BitNot => {
                    ctx.emit_here(IrInst::BitNot {
                        result: result_ty,
                        prim_kind: prim_kind_byte(ctx.typed, result_ty),
                    });
                }
                UnaryOp::Deref => {
                    let operand_ty = ctx
                        .typed
                        .expr_types
                        .get(&operand_id)
                        .copied()
                        .unwrap_or(result_ty);
                    lower_ptr_deref(ctx, operand, operand_ty, result_ty);
                }
                UnaryOp::Ref | UnaryOp::RefMut => {
                    // `AddressOfLocal` emitted above; operand already consumed.
                }
            }
        }
        Expr::Binary { op, left, right } => {
            lower_binary(ctx, *op, left, right, result_ty);
        }
        Expr::Assign { target, value, .. } => {
            lower_assign_expr(ctx, target, value, result_ty);
        }
        Expr::Cast { expr, .. } => {
            let from_ty = ctx.peek_next_expr_ty();
            if matches!(ctx.typed.types.get(result_ty), Ty::Str) {
                if let Some(bytes) = utf8_bytes_for_str_cast(ctx, &expr.inner) {
                    let idx = ctx.intern_const(crate::ir::IrConst::Bytes(bytes));
                    let _ = ctx.expr_ty();
                    ctx.emit_here(IrInst::MakeStr { pool_index: idx });
                    return;
                }
            }
            lower_expr(ctx, expr);
            if from_ty != result_ty && !tuple_struct_repr_cast(ctx, from_ty, result_ty) {
                if let (Ty::Array { elem, .. }, Ty::Slice(slice_elem)) =
                    (ctx.typed.types.get(from_ty), ctx.typed.types.get(result_ty))
                {
                    if elem == slice_elem {
                        let elem_kind = primitive_kind_for_type(&ctx.typed.types, *elem)
                            .map_or(SLOT_KIND_AGG, phx_bytecode::PrimitiveKind::as_u8);
                        ctx.emit_here(IrInst::MakeSlice { elem_kind });
                    }
                } else if matches!(ctx.typed.types.get(from_ty), Ty::Str) {
                    if let Ty::Slice(inner) = ctx.typed.types.get(result_ty) {
                        if matches!(
                            ctx.typed.types.get(*inner),
                            Ty::Primitive(phx_syntax::token::Keyword::U8)
                        ) {
                            ctx.emit_here(IrInst::StrAsSlice);
                        }
                    }
                } else if let (Some(from_k), Some(to_k)) = (
                    primitive_kind_for_type(&ctx.typed.types, from_ty),
                    primitive_kind_for_type(&ctx.typed.types, result_ty),
                ) {
                    ctx.emit_here(IrInst::Cast {
                        from_kind: from_k.as_u8(),
                        to_kind: to_k.as_u8(),
                    });
                }
            }
        }
        Expr::Postfix { base, ops } => lower_postfix(ctx, base, ops, result_ty, expr_id),
        Expr::If {
            condition,
            then_block,
            else_ifs,
            else_block,
        } => lower_if(
            ctx,
            condition.as_ref(),
            then_block,
            else_ifs,
            else_block.as_ref(),
        ),
        Expr::Match { scrutinee, arms } => lower_match(ctx, scrutinee, arms),
        Expr::Block(block) => lower_block_expr(ctx, block),
        Expr::StructLit { name, fields, .. } => {
            if let Some(def) = struct_def_by_name(&ctx.typed.resolved, name.symbol) {
                let (type_def, args) = match ctx.typed.types.get(result_ty) {
                    Ty::Named { def, args } => (*def, args.clone()),
                    _ => (def, Vec::new()),
                };
                if let Some((type_id, struct_layout)) =
                    crate::lower::ctx::struct_lit_layout_ops(&ctx.typed.layout, type_def, &args)
                {
                    let struct_fields = Some(struct_layout.fields.as_slice());
                    let mut field_count = 0u32;
                    for field in fields {
                        if let StructFieldInit::Field { name, value, .. } = field {
                            let fty = struct_fields
                                .and_then(|fs| {
                                    fs.iter()
                                        .find(|(sym, _)| *sym == name.symbol)
                                        .map(|(_, ty)| *ty)
                                })
                                .unwrap_or(result_ty);
                            lower_expr_with_type(ctx, value, fty);
                            field_count = field_count.saturating_add(1);
                        }
                    }
                    ctx.emit_here(IrInst::MakeStruct {
                        type_id,
                        field_count,
                    });
                }
            } else if let Some((enum_def, variant)) =
                ctx.typed.layout.enum_variant_by_name(name.symbol)
            {
                let args = match ctx.typed.types.get(result_ty) {
                    Ty::Named { def, args } if *def == enum_def => args.clone(),
                    _ => Vec::new(),
                };
                let payload = ctx
                    .typed
                    .layout
                    .enum_layout(enum_def, &args)
                    .and_then(|el| {
                        el.variants
                            .iter()
                            .find(|v| v.name == name.symbol)
                            .map(|v| v.kind.clone())
                    })
                    .unwrap_or_else(|| variant.kind.clone());
                if let crate::typeck::VariantKind::Struct(payload) = payload {
                    let Some(type_id) = ctx.require_type_id_for_named(enum_def, &args, name.span)
                    else {
                        return;
                    };
                    for (fname, _) in &payload {
                        if let Some(StructFieldInit::Field { value, .. }) =
                            fields.iter().find(|f| {
                                matches!(
                                    f,
                                    StructFieldInit::Field { name: n, .. }
                                        if n.symbol == *fname
                                )
                            })
                        {
                            lower_expr(ctx, value);
                        }
                    }
                    let payload_count = u32::try_from(payload.len()).unwrap_or(u32::MAX);
                    ctx.emit_here(IrInst::MakeEnum {
                        type_id,
                        variant_tag: variant.tag,
                        payload_count,
                    });
                }
            }
        }
        Expr::Unsafe(block) => lower_block_expr(ctx, block),
        // Post-MVP / grammar-deferred — rejected by typeck in MVP (`grammar-deferred.md`).
        Expr::Range { .. } | Expr::Lambda { .. } | Expr::RuntimeDirective { .. } => {}
    }
}

pub(super) fn named_type_parts(typed: &TypedProgram, ty: TypeId) -> Option<(DefId, Vec<TypeId>)> {
    let mut seen = std::collections::HashSet::new();
    named_type_parts_inner(typed, ty, &mut seen)
}

fn named_type_parts_inner(
    typed: &TypedProgram,
    ty: TypeId,
    seen: &mut std::collections::HashSet<DefId>,
) -> Option<(DefId, Vec<TypeId>)> {
    match typed.types.get(ty).clone() {
        Ty::Named { def, args } => {
            if seen.insert(def) {
                if typed
                    .resolved
                    .defs
                    .get(def.index() as usize)
                    .is_some_and(|d| d.kind == DefKind::TypeAlias)
                    && let Some(&alias_ty) = typed.value_types.get(&def)
                    && let Some(peeled) = named_type_parts_inner(typed, alias_ty, seen)
                {
                    return Some(peeled);
                }
            }
            Some((def, args))
        }
        Ty::Ref { inner, .. } => named_type_parts_inner(typed, inner, seen),
        _ => None,
    }
}

pub(super) fn tuple_struct_repr_cast(ctx: &LowerCtx<'_>, from: TypeId, to: TypeId) -> bool {
    single_field_tuple_inner(ctx, from).is_some_and(|inner| types_same_for_cast(ctx, inner, to))
        || single_field_tuple_inner(ctx, to)
            .is_some_and(|inner| types_same_for_cast(ctx, inner, from))
}

fn types_same_for_cast(_ctx: &LowerCtx<'_>, a: TypeId, b: TypeId) -> bool {
    a == b
}

fn single_field_tuple_inner(ctx: &LowerCtx<'_>, ty: TypeId) -> Option<TypeId> {
    let (def, args) = named_type_parts(ctx.typed, ty)?;
    if !ctx.typed.layout.tuple_structs.contains(&def) {
        return None;
    }
    let layout = ctx.typed.layout.struct_layout(def, &args)?;
    if layout.fields.len() != 1 {
        return None;
    }
    Some(layout.fields[0].1)
}

pub(super) fn aggregate_deref_target(typed: &TypedProgram, ty: TypeId) -> bool {
    match typed.types.get(ty) {
        Ty::Named { def, .. } => typed
            .resolved
            .defs
            .get(def.index() as usize)
            .is_some_and(|d| matches!(d.kind, DefKind::Struct | DefKind::Enum)),
        Ty::Tuple(_) | Ty::Array { .. } => true,
        _ => false,
    }
}

fn lower_ptr_deref(
    ctx: &mut LowerCtx<'_>,
    operand: &ExprNode,
    operand_ty: TypeId,
    result_ty: TypeId,
) {
    let load_ty = deref_load_type(ctx, operand, operand_ty, result_ty);
    if let Some(kind) = primitive_kind_for_type(&ctx.typed.types, load_ty) {
        ctx.emit_here(IrInst::PtrLoad {
            prim_kind: kind.as_u8(),
            signed: primitive_load_signed(kind),
            result: load_ty,
        });
    } else if aggregate_deref_target(ctx.typed, load_ty) {
        ctx.emit_here(IrInst::LoadAggViaLocalPtr);
    }
}

fn deref_load_type(
    ctx: &LowerCtx<'_>,
    operand: &ExprNode,
    operand_ty: TypeId,
    result_ty: TypeId,
) -> TypeId {
    if let Some(pointee) = prim::pointee_type_for_deref_operand(ctx, operand, operand_ty) {
        return pointee;
    }
    if primitive_kind_for_type(&ctx.typed.types, result_ty).is_some() {
        return result_ty;
    }
    if let Ty::Ptr { inner, .. } = ctx.typed.types.get(operand_ty) {
        if primitive_kind_for_type(&ctx.typed.types, *inner).is_some() {
            return *inner;
        }
        if primitive_kind_for_type(&ctx.typed.types, ctx.layout.return_type).is_some() {
            return ctx.layout.return_type;
        }
    }
    if aggregate_deref_target(ctx.typed, result_ty) {
        return result_ty;
    }
    if primitive_kind_for_type(&ctx.typed.types, ctx.layout.return_type).is_some() {
        return ctx.layout.return_type;
    }
    match ctx.typed.types.get(operand_ty) {
        Ty::Ptr { inner, .. } => *inner,
        _ => result_ty,
    }
}

pub(super) fn field_result_ty(ctx: &LowerCtx<'_>, struct_def: DefId, field: Symbol) -> TypeId {
    ctx.typed
        .layout
        .structs
        .get(&struct_def)
        .and_then(|sl| {
            sl.fields
                .iter()
                .find(|(name, _)| *name == field)
                .map(|(_, ty)| *ty)
        })
        .unwrap_or_else(|| crate::lower::ctx::unit_ty(ctx.typed))
}

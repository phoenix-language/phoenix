//! `if`, `match`, and block expressions.

use phx_syntax::Symbol;
use phx_syntax::ast::expr::{ExprNode, IfCondition};
use phx_syntax::ast::pat::{MatchArm, Pattern};
use phx_syntax::ast::stmt::BlockNode;

use crate::ir::{IrBinOp, IrInst};
use crate::lower::ctx::{LowerCtx, bool_ty, prim_kind_byte, struct_def_by_name, unit_ty};
use crate::typeck::{LocalSlot, Ty, TypeId, VariantKind};

use super::literal::intern_literal;
use super::{field_result_ty, lower_expr};

pub(super) fn lower_if(
    ctx: &mut LowerCtx<'_>,
    condition: &IfCondition,
    then_block: &BlockNode,
    else_ifs: &[(IfCondition, BlockNode)],
    else_block: Option<&BlockNode>,
) {
    let entry = ctx.current;
    let then_id = ctx.fresh_block();
    let else_id = ctx.fresh_block();
    let merge_id = ctx.fresh_block();
    let pattern_arm = lower_if_condition_test(ctx, condition, then_id, else_id);

    ctx.set_current(then_id);
    if let Some((pattern, temp, temp_ty)) = pattern_arm {
        bind_match_pattern(ctx, &pattern.inner, temp, temp_ty, false);
    }
    lower_block_expr(ctx, then_block);
    if !block_ends_with_unconditional_jump(ctx, then_id) {
        ctx.emit(IrInst::Jump { target: merge_id });
    }

    ctx.set_current(else_id);
    if else_ifs.is_empty() {
        if let Some(else_b) = else_block {
            lower_block_expr(ctx, else_b);
        }
    } else {
        lower_else_if_chain(ctx, else_ifs, else_block);
    }
    if !block_ends_with_unconditional_jump(ctx, else_id) {
        ctx.emit(IrInst::Jump { target: merge_id });
    }

    ctx.set_current(merge_id);
    let _ = entry;
}

/// Emits control flow for one `if` / `else if` condition. Returns pattern metadata when binding.
pub(super) fn lower_if_condition_test(
    ctx: &mut LowerCtx<'_>,
    condition: &IfCondition,
    then_id: u32,
    else_id: u32,
) -> Option<(phx_syntax::ast::pat::PatternNode, LocalSlot, TypeId)> {
    match condition {
        IfCondition::Bool(cond) => {
            lower_expr(ctx, cond);
            ctx.emit(IrInst::JumpIf {
                then_block: then_id,
                else_block: else_id,
            });
            None
        }
        IfCondition::Pattern {
            pattern, scrutinee, ..
        } => {
            let temp = ctx.next_match_temp();
            let temp_ty = ctx
                .layout
                .bindings
                .iter()
                .find(|b| b.slot == temp)
                .map_or_else(|| unit_ty(ctx.typed), |b| b.ty);
            lower_expr(ctx, scrutinee);
            ctx.emit(IrInst::StoreLocal {
                slot: temp,
                ty: temp_ty,
                prim_kind: prim_kind_byte(ctx.typed, temp_ty),
            });
            emit_arm_condition(ctx, &pattern.inner, temp, temp_ty, then_id, else_id);
            Some((pattern.clone(), temp, temp_ty))
        }
    }
}

/// True when `block` ends with an unconditional branch (no fall-through to merge).
pub(crate) fn block_ends_with_unconditional_jump(ctx: &LowerCtx<'_>, block: u32) -> bool {
    let idx = usize::try_from(block).ok();
    let Some(b) = idx.and_then(|i| ctx.blocks.get(i)) else {
        return false;
    };
    b.insts
        .last()
        .is_some_and(|inst| matches!(inst, IrInst::Jump { .. } | IrInst::Return { .. }))
}

fn lower_else_if_chain(
    ctx: &mut LowerCtx<'_>,
    else_ifs: &[(IfCondition, BlockNode)],
    final_else: Option<&BlockNode>,
) {
    let (first_cond, first_block) = &else_ifs[0];
    let rest = &else_ifs[1..];
    let then_id = ctx.fresh_block();
    let else_id = ctx.fresh_block();
    let merge_id = ctx.fresh_block();
    let pattern_arm = lower_if_condition_test(ctx, first_cond, then_id, else_id);

    ctx.set_current(then_id);
    if let Some((pattern, temp, temp_ty)) = pattern_arm {
        bind_match_pattern(ctx, &pattern.inner, temp, temp_ty, false);
    }
    lower_block_expr(ctx, first_block);
    if !block_ends_with_unconditional_jump(ctx, then_id) {
        ctx.emit(IrInst::Jump { target: merge_id });
    }

    ctx.set_current(else_id);
    if rest.is_empty() {
        if let Some(else_b) = final_else {
            lower_block_expr(ctx, else_b);
        }
    } else {
        lower_else_if_chain(ctx, rest, final_else);
    }
    if !block_ends_with_unconditional_jump(ctx, else_id) {
        ctx.emit(IrInst::Jump { target: merge_id });
    }

    ctx.set_current(merge_id);
}

pub(super) fn lower_match(ctx: &mut LowerCtx<'_>, scrutinee: &ExprNode, arms: &[MatchArm]) {
    if arms.is_empty() {
        ctx.emit(IrInst::TrapGivenMismatch);
        return;
    }

    let temp = ctx.next_match_temp();
    let temp_ty = ctx
        .layout
        .bindings
        .iter()
        .find(|b| b.slot == temp)
        .map_or_else(|| unit_ty(ctx.typed), |b| b.ty);

    lower_expr(ctx, scrutinee);
    ctx.emit(IrInst::StoreLocal {
        slot: temp,
        ty: temp_ty,
        prim_kind: prim_kind_byte(ctx.typed, temp_ty),
    });

    let mut test_blocks = Vec::with_capacity(arms.len());
    let mut body_blocks = Vec::with_capacity(arms.len());
    for _ in arms {
        test_blocks.push(ctx.fresh_block());
        body_blocks.push(ctx.fresh_block());
    }
    let trap_id = ctx.fresh_block();
    let merge_id = ctx.fresh_block();

    ctx.emit(IrInst::Jump {
        target: test_blocks[0],
    });

    for (i, arm) in arms.iter().enumerate() {
        let fail_id = if i + 1 == arms.len() {
            trap_id
        } else {
            test_blocks[i + 1]
        };
        ctx.set_current(test_blocks[i]);
        emit_arm_condition(
            ctx,
            &arm.pattern.inner,
            temp,
            temp_ty,
            body_blocks[i],
            fail_id,
        );

        ctx.set_current(body_blocks[i]);
        bind_match_pattern(ctx, &arm.pattern.inner, temp, temp_ty, false);
        if let Some(guard) = &arm.guard {
            let guarded_body = ctx.fresh_block();
            lower_expr(ctx, guard);
            ctx.emit(IrInst::JumpIf {
                then_block: guarded_body,
                else_block: fail_id,
            });
            ctx.set_current(guarded_body);
        }
        lower_expr(ctx, &arm.body);
        ctx.emit(IrInst::Jump { target: merge_id });
    }

    ctx.set_current(trap_id);
    ctx.emit(IrInst::TrapGivenMismatch);

    ctx.set_current(merge_id);
}

/// Emits branch tests for one `match` arm (tag compare, bindings, guard).
pub(crate) fn emit_arm_condition(
    ctx: &mut LowerCtx<'_>,
    pat: &Pattern,
    temp: LocalSlot,
    temp_ty: TypeId,
    body_id: u32,
    fail_id: u32,
) {
    match pat {
        Pattern::Wildcard => {
            ctx.emit(IrInst::Jump { target: body_id });
        }
        Pattern::Ident(ident) => {
            if let Some((type_id, tag, _)) = enum_variant_for_scrutinee(ctx, temp_ty, ident.symbol)
            {
                ctx.emit(IrInst::LoadLocal {
                    slot: temp,
                    ty: temp_ty,
                    prim_kind: prim_kind_byte(ctx.typed, temp_ty),
                });
                ctx.emit(IrInst::MatchTag {
                    type_id,
                    variant_tag: tag,
                });
                ctx.emit(IrInst::JumpIf {
                    then_block: body_id,
                    else_block: fail_id,
                });
            } else {
                ctx.emit(IrInst::Jump { target: body_id });
            }
        }
        Pattern::Literal(lit) => {
            ctx.emit(IrInst::LoadLocal {
                slot: temp,
                ty: temp_ty,
                prim_kind: prim_kind_byte(ctx.typed, temp_ty),
            });
            let index = intern_literal(ctx, lit, temp_ty);
            let bool_id = bool_ty(ctx.typed);
            ctx.emit(IrInst::Const {
                index,
                ty: temp_ty,
                prim_kind: prim_kind_byte(ctx.typed, temp_ty),
            });
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Eq,
                result: bool_id,
                prim_kind: prim_kind_byte(ctx.typed, bool_id),
            });
            ctx.emit(IrInst::JumpIf {
                then_block: body_id,
                else_block: fail_id,
            });
        }
        Pattern::Struct { name, .. } => {
            if let Some((type_id, tag, _)) = enum_variant_for_scrutinee(ctx, temp_ty, name.symbol) {
                ctx.emit(IrInst::LoadLocal {
                    slot: temp,
                    ty: temp_ty,
                    prim_kind: prim_kind_byte(ctx.typed, temp_ty),
                });
                ctx.emit(IrInst::MatchTag {
                    type_id,
                    variant_tag: tag,
                });
                ctx.emit(IrInst::JumpIf {
                    then_block: body_id,
                    else_block: fail_id,
                });
            } else if struct_def_by_name(&ctx.typed.resolved, name.symbol).is_some() {
                ctx.emit(IrInst::Jump { target: body_id });
            } else {
                ctx.emit(IrInst::Jump { target: fail_id });
            }
        }
        Pattern::Tuple { name, .. } => {
            if let Some((type_id, tag, _)) = enum_variant_for_scrutinee(ctx, temp_ty, name.symbol) {
                ctx.emit(IrInst::LoadLocal {
                    slot: temp,
                    ty: temp_ty,
                    prim_kind: prim_kind_byte(ctx.typed, temp_ty),
                });
                ctx.emit(IrInst::MatchTag {
                    type_id,
                    variant_tag: tag,
                });
                ctx.emit(IrInst::JumpIf {
                    then_block: body_id,
                    else_block: fail_id,
                });
            } else {
                ctx.emit(IrInst::Jump { target: fail_id });
            }
        }
        Pattern::Range { .. } => {
            ctx.emit(IrInst::Jump { target: fail_id });
        }
    }
}

fn enum_variant_for_scrutinee(
    ctx: &LowerCtx<'_>,
    scrutinee_ty: TypeId,
    variant_name: Symbol,
) -> Option<(u32, u32, VariantKind)> {
    let Ty::Named { def, args } = ctx.typed.types.get(scrutinee_ty).clone() else {
        return None;
    };
    let layout = ctx.typed.layout.enum_layout(def, &args)?;
    let variant = layout.variants.iter().find(|v| v.name == variant_name)?;
    let type_id = ctx.typed.layout.type_id_for_named(def, &args)?;
    Some((type_id, variant.tag, variant.kind.clone()))
}

/// Binds `match` pattern variables into locals and moves payload slots when needed.
#[allow(clippy::too_many_lines)]
pub(crate) fn bind_match_pattern(
    ctx: &mut LowerCtx<'_>,
    pat: &Pattern,
    temp: LocalSlot,
    temp_ty: TypeId,
    value_on_stack: bool,
) {
    match pat {
        Pattern::Ident(ident) => {
            if enum_variant_for_scrutinee(ctx, temp_ty, ident.symbol).is_some() {
                return;
            }
            if let Some(binding) = ctx.layout.binding(ident.symbol) {
                if !value_on_stack {
                    ctx.emit(IrInst::LoadLocal {
                        slot: temp,
                        ty: temp_ty,
                        prim_kind: prim_kind_byte(ctx.typed, temp_ty),
                    });
                }
                ctx.emit(IrInst::StoreLocal {
                    slot: binding.slot,
                    ty: binding.ty,
                    prim_kind: prim_kind_byte(ctx.typed, binding.ty),
                });
            }
        }
        Pattern::Struct { name, fields } => {
            if let Some(def) = struct_def_by_name(&ctx.typed.resolved, name.symbol) {
                let type_id = ctx.typed.layout.type_id(def).unwrap_or(0);
                for field in fields {
                    let field_index = ctx
                        .typed
                        .layout
                        .struct_field_index(def, field.name.symbol, &[])
                        .unwrap_or(0);
                    ctx.emit(IrInst::LoadLocal {
                        slot: temp,
                        ty: temp_ty,
                        prim_kind: prim_kind_byte(ctx.typed, temp_ty),
                    });
                    let result_ty = field_result_ty(ctx, def, field.name.symbol);
                    ctx.emit(IrInst::GetField {
                        type_id,
                        field_index,
                        result: result_ty,
                    });
                    if let Some(p) = &field.pattern {
                        bind_match_pattern(ctx, &p.inner, temp, result_ty, true);
                    } else if let Some(binding) = ctx.layout.binding(field.name.symbol) {
                        ctx.emit(IrInst::StoreLocal {
                            slot: binding.slot,
                            ty: result_ty,
                            prim_kind: prim_kind_byte(ctx.typed, result_ty),
                        });
                    }
                }
            } else if let Some((type_id, _tag, VariantKind::Struct(fields_payload))) =
                enum_variant_for_scrutinee(ctx, temp_ty, name.symbol)
            {
                for (i, (fname, fty)) in fields_payload.iter().enumerate() {
                    let Some(pat_field) = fields.iter().find(|f| f.name.symbol == *fname) else {
                        continue;
                    };
                    ctx.emit(IrInst::LoadLocal {
                        slot: temp,
                        ty: temp_ty,
                        prim_kind: prim_kind_byte(ctx.typed, temp_ty),
                    });
                    let field_index = u32::try_from(i).unwrap_or(u32::MAX);
                    ctx.emit(IrInst::GetField {
                        type_id,
                        field_index,
                        result: *fty,
                    });
                    if let Some(p) = &pat_field.pattern {
                        bind_match_pattern(ctx, &p.inner, temp, *fty, true);
                    } else if let Some(binding) = ctx.layout.binding(pat_field.name.symbol) {
                        ctx.emit(IrInst::StoreLocal {
                            slot: binding.slot,
                            ty: *fty,
                            prim_kind: prim_kind_byte(ctx.typed, *fty),
                        });
                    }
                }
            }
        }
        Pattern::Tuple { name, patterns } => {
            if let Some((type_id, _tag, VariantKind::Tuple(field_types))) =
                enum_variant_for_scrutinee(ctx, temp_ty, name.symbol)
            {
                for (i, p) in patterns.iter().enumerate() {
                    ctx.emit(IrInst::LoadLocal {
                        slot: temp,
                        ty: temp_ty,
                        prim_kind: prim_kind_byte(ctx.typed, temp_ty),
                    });
                    let result_ty = field_types
                        .get(i)
                        .copied()
                        .unwrap_or_else(|| unit_ty(ctx.typed));
                    ctx.emit(IrInst::GetField {
                        type_id,
                        field_index: u32::try_from(i).unwrap_or(u32::MAX),
                        result: result_ty,
                    });
                    bind_match_pattern(ctx, &p.inner, temp, result_ty, true);
                }
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) => {}
        Pattern::Range { .. } => {}
    }
}

pub(super) fn lower_block_expr(ctx: &mut LowerCtx<'_>, block: &BlockNode) {
    crate::lower::stmt::lower_block_value(ctx, &block.inner);
}

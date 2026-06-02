//! Lower expressions to IR instructions (stack-oriented).

use phx_syntax::ast::expr::{BinOp, Expr, ExprNode, PostfixOp};
use phx_syntax::ast::ident::{Ident, Path, PathSegment};
use phx_syntax::ast::lit::Literal;
use phx_syntax::ast::pat::{MatchArm, Pattern};
use phx_syntax::ast::stmt::BlockNode;

use crate::ir::{IrBinOp, IrInst};
use crate::lower::ctx::{
    LowerCtx, bool_ty, const_index_for_literal, lookup_resolution, slot_for_symbol, unit_ty,
};
use crate::resolver::DefId;
use crate::typeck::TypeId;

/// Lowers `expr` so its value is on the implicit stack.
pub fn lower_expr(ctx: &mut LowerCtx<'_>, expr: &ExprNode) {
    let ty = ctx.expr_ty();
    lower_expr_inner(ctx, &expr.inner, ty);
}

fn lower_expr_inner(ctx: &mut LowerCtx<'_>, expr: &Expr, result_ty: TypeId) {
    match expr {
        Expr::Literal(lit) => lower_literal(ctx, lit, result_ty),
        Expr::Ident(ident) => lower_ident(ctx, *ident, result_ty),
        Expr::Path(path) => lower_path(ctx, path, result_ty),
        Expr::Tuple(items) => {
            for item in items {
                lower_expr(ctx, item);
            }
        }
        Expr::Array(items) => {
            for item in items {
                lower_expr(ctx, item);
            }
        }
        Expr::Unary { operand, .. } => {
            lower_expr(ctx, operand);
        }
        Expr::Binary { op, left, right } => {
            lower_binary(ctx, *op, left, right, result_ty);
        }
        Expr::Assign { target, value, .. } => {
            lower_assign_expr(ctx, target, value);
        }
        Expr::Cast { expr, .. } => {
            lower_expr(ctx, expr);
        }
        Expr::Postfix { base, ops } => lower_postfix(ctx, base, ops, result_ty),
        Expr::If {
            cond,
            then_block,
            else_ifs,
            else_block,
        } => lower_if(ctx, cond, then_block, else_ifs, else_block.as_ref()),
        Expr::Match { scrutinee, arms } => lower_match(ctx, scrutinee, arms),
        Expr::Block(block) => lower_block_expr(ctx, block),
        Expr::StructLit { fields, .. } => {
            for field in fields {
                match field {
                    phx_syntax::ast::expr::StructFieldInit::Field { value, .. } => {
                        lower_expr(ctx, value);
                    }
                    phx_syntax::ast::expr::StructFieldInit::Spread(base) => {
                        lower_expr(ctx, base);
                    }
                    _ => {}
                }
            }
        }
        Expr::Unsafe(block) => lower_block_expr(ctx, block),
        _ => {}
    }
}

fn lower_literal(ctx: &mut LowerCtx<'_>, lit: &Literal, ty: TypeId) {
    if let Some(index) = const_index_for_literal(lit) {
        ctx.emit(IrInst::Const { index, ty });
    }
}

fn lower_ident(ctx: &mut LowerCtx<'_>, ident: Ident, ty: TypeId) {
    if let Some(slot) = slot_for_symbol(ctx.layout, ident.symbol) {
        ctx.emit(IrInst::LoadLocal { slot, ty });
    }
}

fn lower_path(ctx: &mut LowerCtx<'_>, path: &Path, ty: TypeId) {
    if path.segments.len() == 1
        && let PathSegment::Ident(ident) = path.segments[0]
    {
        lower_ident(ctx, ident, ty);
    }
}

fn lower_binary(
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
            });
        }
        BinOp::Ge => {
            lower_expr(ctx, right);
            lower_expr(ctx, left);
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Lt,
                result: result_ty,
            });
        }
        BinOp::Le => {
            lower_expr(ctx, left);
            lower_expr(ctx, right);
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Lt,
                result: result_ty,
            });
        }
        BinOp::Ne => {
            lower_expr(ctx, left);
            lower_expr(ctx, right);
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Eq,
                result: result_ty,
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
        }
        _ => {
            lower_expr(ctx, left);
            lower_expr(ctx, right);
        }
    }
}

fn lower_short_circuit_bool(
    ctx: &mut LowerCtx<'_>,
    op: BinOp,
    left: &ExprNode,
    right: &ExprNode,
    result_ty: TypeId,
) {
    let entry = ctx.current;
    lower_expr(ctx, left);

    let rhs_id = ctx.fresh_block();
    let short_id = ctx.fresh_block();
    let merge_id = ctx.fresh_block();

    ctx.set_current(entry);
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
    let short_val = if op == BinOp::And { 0u32 } else { 1u32 };
    ctx.emit(IrInst::Const {
        index: short_val,
        ty: result_ty,
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
        BinOp::Or
        | BinOp::And
        | BinOp::Gt
        | BinOp::Ge
        | BinOp::Le
        | BinOp::Ne
        | BinOp::BitOr
        | BinOp::BitXor
        | BinOp::BitAnd
        | BinOp::Shl
        | BinOp::Shr
        | BinOp::Mod
        | BinOp::Pow => None,
        _ => None,
    }
}

pub(crate) fn lower_assign_expr(ctx: &mut LowerCtx<'_>, target: &ExprNode, value: &ExprNode) {
    lower_assign_target(ctx, &target.inner);
    lower_expr(ctx, value);
    if let Expr::Ident(ident) = &target.inner
        && let Some(binding) = ctx.layout.binding(ident.symbol)
    {
        ctx.emit(IrInst::StoreLocal {
            slot: binding.slot,
            ty: binding.ty,
        });
    }
}

fn lower_assign_target(ctx: &mut LowerCtx<'_>, target: &Expr) {
    match target {
        Expr::Ident(_) => {}
        Expr::Postfix { base, ops } if ops.len() == 1 => {
            if let PostfixOp::Field(_) = &ops[0] {
                lower_expr(ctx, base);
            }
        }
        Expr::Literal(_) | Expr::Path(_) => {}
        _ => {}
    }
}

fn lower_postfix(ctx: &mut LowerCtx<'_>, base: &ExprNode, ops: &[PostfixOp], result_ty: TypeId) {
    lower_postfix_inner(ctx, base, ops, result_ty);
}

fn lower_postfix_inner(
    ctx: &mut LowerCtx<'_>,
    base: &ExprNode,
    ops: &[PostfixOp],
    result_ty: TypeId,
) {
    lower_expr(ctx, base);
    for op in ops {
        match op {
            PostfixOp::Field(_) => {}
            PostfixOp::Method { args, .. } => {
                for arg in args {
                    lower_expr(ctx, arg);
                }
            }
            PostfixOp::Call(args) => {
                let callee = if let Expr::Ident(ident) = &base.inner {
                    lookup_resolution(&ctx.typed.resolved, base.span, ident.symbol)
                        .unwrap_or(DefId::from_raw(0))
                } else {
                    DefId::from_raw(0)
                };
                for arg in args {
                    lower_expr(ctx, arg);
                }
                ctx.emit(IrInst::Call {
                    callee,
                    ret: result_ty,
                });
            }
            PostfixOp::Index(idx) => {
                lower_expr(ctx, idx);
            }
            PostfixOp::Try => {}
            _ => {}
        }
    }
}

fn lower_if(
    ctx: &mut LowerCtx<'_>,
    cond: &ExprNode,
    then_block: &BlockNode,
    else_ifs: &[(ExprNode, BlockNode)],
    else_block: Option<&BlockNode>,
) {
    let entry = ctx.current;
    lower_expr(ctx, cond);
    let then_id = ctx.fresh_block();
    let else_id = ctx.fresh_block();
    let merge_id = ctx.fresh_block();
    ctx.set_current(entry);
    ctx.emit(IrInst::JumpIf {
        then_block: then_id,
        else_block: else_id,
    });

    ctx.set_current(then_id);
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
    else_ifs: &[(ExprNode, BlockNode)],
    final_else: Option<&BlockNode>,
) {
    let (first_cond, first_block) = &else_ifs[0];
    let rest = &else_ifs[1..];
    let entry = ctx.current;
    lower_expr(ctx, first_cond);
    let then_id = ctx.fresh_block();
    let else_id = ctx.fresh_block();
    let merge_id = ctx.fresh_block();
    ctx.set_current(entry);
    ctx.emit(IrInst::JumpIf {
        then_block: then_id,
        else_block: else_id,
    });

    ctx.set_current(then_id);
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

fn lower_match(ctx: &mut LowerCtx<'_>, scrutinee: &ExprNode, arms: &[MatchArm]) {
    if arms.is_empty() {
        return;
    }

    let temp = ctx.next_match_temp();
    let temp_ty = ctx
        .layout
        .bindings
        .iter()
        .find(|b| b.slot == temp)
        .map(|b| b.ty)
        .unwrap_or_else(|| unit_ty(ctx.typed));

    lower_expr(ctx, scrutinee);
    ctx.emit(IrInst::StoreLocal {
        slot: temp,
        ty: temp_ty,
    });

    let mut test_blocks = Vec::with_capacity(arms.len());
    let mut body_blocks = Vec::with_capacity(arms.len());
    for _ in arms {
        test_blocks.push(ctx.fresh_block());
        body_blocks.push(ctx.fresh_block());
    }
    let merge_id = ctx.fresh_block();

    ctx.emit(IrInst::Jump {
        target: test_blocks[0],
    });

    for (i, arm) in arms.iter().enumerate() {
        let fail_id = if i + 1 == arms.len() {
            merge_id
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
        bind_ident_pattern(ctx, &arm.pattern.inner, temp, temp_ty);
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

    ctx.set_current(merge_id);
}

fn emit_arm_condition(
    ctx: &mut LowerCtx<'_>,
    pat: &Pattern,
    temp: crate::typeck::LocalSlot,
    temp_ty: TypeId,
    body_id: u32,
    fail_id: u32,
) {
    match pat {
        Pattern::Wildcard | Pattern::Ident(_) => {
            ctx.emit(IrInst::Jump { target: body_id });
        }
        Pattern::Literal(lit) => {
            ctx.emit(IrInst::LoadLocal {
                slot: temp,
                ty: temp_ty,
            });
            if let Some(index) = const_index_for_literal(lit) {
                ctx.emit(IrInst::Const { index, ty: temp_ty });
                ctx.emit(IrInst::BinOp {
                    op: IrBinOp::Eq,
                    result: bool_ty(ctx.typed),
                });
                ctx.emit(IrInst::JumpIf {
                    then_block: body_id,
                    else_block: fail_id,
                });
            } else {
                ctx.emit(IrInst::Jump { target: fail_id });
            }
        }
        Pattern::Struct { .. } | Pattern::Tuple { .. } => {
            ctx.emit(IrInst::Jump { target: fail_id });
        }
        _ => {
            ctx.emit(IrInst::Jump { target: fail_id });
        }
    }
}

fn bind_ident_pattern(
    ctx: &mut LowerCtx<'_>,
    pat: &Pattern,
    temp: crate::typeck::LocalSlot,
    temp_ty: TypeId,
) {
    if let Pattern::Ident(ident) = pat
        && let Some(binding) = ctx.layout.binding(ident.symbol)
    {
        ctx.emit(IrInst::LoadLocal {
            slot: temp,
            ty: temp_ty,
        });
        ctx.emit(IrInst::StoreLocal {
            slot: binding.slot,
            ty: temp_ty,
        });
    }
}

fn lower_block_expr(ctx: &mut LowerCtx<'_>, block: &BlockNode) {
    crate::lower::stmt::lower_block_value(ctx, &block.inner);
}

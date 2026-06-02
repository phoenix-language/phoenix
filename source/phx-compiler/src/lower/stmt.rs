//! Lower statements and blocks to IR control flow.

use phx_syntax::ast::expr::Expr;
use phx_syntax::ast::pat::PatternNode;
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};

use crate::ir::IrInst;
use crate::lower::ctx::{LoopLabels, LowerCtx, prim_kind_byte, unit_ty};
use crate::lower::expr::{
    bind_match_pattern, block_ends_with_unconditional_jump, emit_arm_condition, lower_assign_expr,
    lower_expr,
};
use crate::typeck::TypeId;

/// Lowers `block` for its trailing value (expression body or last item).
pub fn lower_block_value(ctx: &mut LowerCtx<'_>, block: &Block) {
    for item in &block.items {
        match item {
            BlockItem::Stmt(stmt) => lower_block_stmt(ctx, stmt),
            BlockItem::Expr(expr) => lower_expr(ctx, expr),
            _ => {}
        }
    }
}

fn lower_block_stmt(ctx: &mut LowerCtx<'_>, stmt: &Stmt) {
    match stmt {
        Stmt::Const { name, init, .. } | Stmt::Var { name, init, .. } => {
            lower_expr(ctx, init);
            if let Some(binding) = ctx.layout.binding(name.symbol) {
                ctx.emit(IrInst::StoreLocal {
                    slot: binding.slot,
                    ty: binding.ty,
                    prim_kind: prim_kind_byte(ctx.typed, binding.ty),
                });
            }
        }
        Stmt::Assign { expr } => {
            if let Expr::Assign { target, value, .. } = &expr.inner {
                lower_assign_expr(ctx, target, value);
            }
        }
        Stmt::Expr(expr) => {
            if let Expr::Assign { target, value, .. } = &expr.inner {
                lower_assign_expr(ctx, target, value);
            } else {
                lower_expr(ctx, expr);
            }
        }
        Stmt::Return(expr) => lower_return(ctx, expr.as_ref()),
        Stmt::While { cond, body } => lower_while(ctx, cond, &body.inner),
        Stmt::Loop(body) => lower_loop(ctx, &body.inner),
        Stmt::Given {
            pattern,
            scrutinee,
            body,
            ..
        } => lower_given(ctx, pattern, scrutinee, &body.inner),
        Stmt::Unsafe(body) => lower_block_value(ctx, &body.inner),
        Stmt::Break(expr) => lower_break(ctx, expr.as_ref()),
        Stmt::Continue => lower_continue(ctx),
        _ => {}
    }
}

/// Lowers `given pat = scrutinee { body }` as a single-arm match with trap on mismatch.
fn lower_given(
    ctx: &mut LowerCtx<'_>,
    pattern: &PatternNode,
    scrutinee: &phx_syntax::ast::ExprNode,
    body: &Block,
) {
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
        prim_kind: prim_kind_byte(ctx.typed, temp_ty),
    });

    let test_id = ctx.fresh_block();
    let body_id = ctx.fresh_block();
    let fail_id = ctx.fresh_block();
    let after_id = ctx.fresh_block();

    ctx.emit(IrInst::Jump { target: test_id });

    ctx.set_current(test_id);
    emit_arm_condition(ctx, &pattern.inner, temp, temp_ty, body_id, fail_id);

    ctx.set_current(body_id);
    bind_match_pattern(ctx, &pattern.inner, temp, temp_ty, false);
    lower_block_value(ctx, body);
    ctx.emit(IrInst::Jump { target: after_id });

    ctx.set_current(fail_id);
    ctx.emit(IrInst::TrapGivenMismatch);

    ctx.set_current(after_id);
}

/// `while (cond) { body }` — header tests `cond`, body jumps back to header.
fn lower_while(ctx: &mut LowerCtx<'_>, cond: &phx_syntax::ast::ExprNode, body: &Block) {
    let header = ctx.fresh_block();
    let body_id = ctx.fresh_block();
    let exit_slot = ctx.alloc_loop_exit_slot();

    ctx.emit(IrInst::Jump { target: header });
    ctx.push_loop(LoopLabels {
        exit_slot,
        continue_target: header,
    });

    ctx.set_current(header);
    lower_expr(ctx, cond);
    ctx.emit(IrInst::JumpIf {
        then_block: body_id,
        else_block: LowerCtx::loop_exit_target(exit_slot),
    });

    ctx.set_current(body_id);
    lower_block_value(ctx, body);
    if !block_ends_with_unconditional_jump(ctx, ctx.current) {
        ctx.emit(IrInst::Jump { target: header });
    }

    ctx.pop_loop();
    let exit = ctx.fresh_block();
    ctx.pending_loop_exits[exit_slot] = Some(exit);
    ctx.set_current(exit);
}

/// `loop { body }` — body repeats until `break` (or `return`).
fn lower_loop(ctx: &mut LowerCtx<'_>, body: &Block) {
    let header = ctx.fresh_block();
    let exit_slot = ctx.alloc_loop_exit_slot();

    ctx.emit(IrInst::Jump { target: header });
    ctx.push_loop(LoopLabels {
        exit_slot,
        continue_target: header,
    });

    ctx.set_current(header);
    lower_block_value(ctx, body);
    if !block_ends_with_unconditional_jump(ctx, ctx.current) {
        ctx.emit(IrInst::Jump { target: header });
    }

    ctx.pop_loop();
    let exit = ctx.fresh_block();
    ctx.pending_loop_exits[exit_slot] = Some(exit);
    ctx.set_current(exit);
}

fn lower_break(ctx: &mut LowerCtx<'_>, expr: Option<&phx_syntax::ast::ExprNode>) {
    if let Some(e) = expr {
        lower_expr(ctx, e);
    }
    if let Some(labels) = ctx.innermost_loop() {
        ctx.emit(IrInst::Jump {
            target: LowerCtx::loop_exit_target(labels.exit_slot),
        });
    }
}

fn lower_continue(ctx: &mut LowerCtx<'_>) {
    if let Some(labels) = ctx.innermost_loop() {
        ctx.emit(IrInst::Jump {
            target: labels.continue_target,
        });
    }
}

fn lower_return(ctx: &mut LowerCtx<'_>, expr: Option<&phx_syntax::ast::ExprNode>) {
    if let Some(e) = expr {
        lower_expr(ctx, e);
        ctx.emit(IrInst::Return {
            ty: ctx.layout.return_type,
        });
    } else {
        ctx.emit(IrInst::Return {
            ty: unit_ty(ctx.typed),
        });
    }
}

/// Emits [`IrInst::Return`] after the function body has been lowered.
pub fn lower_function_return(ctx: &mut LowerCtx<'_>, _body: &Block, return_type: TypeId) {
    ctx.emit(IrInst::Return { ty: return_type });
}

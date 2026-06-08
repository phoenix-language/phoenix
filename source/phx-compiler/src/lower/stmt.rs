//! Lower statements and blocks to IR control flow.

use phx_syntax::ast::expr::Expr;
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};

use crate::ir::IrInst;
use crate::lower::ctx::{LoopLabels, LowerCtx, prim_kind_byte, unit_ty};
use crate::lower::drop_glue::{emit_scope_drops, loop_body_scope_depth};
use crate::lower::expr::{block_ends_with_unconditional_jump, lower_assign_expr, lower_expr};
use crate::typeck::TypeId;

/// Lowers `block` for its trailing value (expression body or last item).
pub fn lower_block_value(ctx: &mut LowerCtx<'_>, block: &Block) {
    ctx.enter_scope();
    for item in &block.items {
        match item {
            BlockItem::Stmt(stmt) => lower_block_stmt(ctx, stmt),
            BlockItem::Expr(expr) => lower_expr(ctx, expr),
            BlockItem::Import(_) => {}
            _ => {}
        }
    }
    ctx.exit_scope();
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
        Stmt::ForIn { .. } => {}
        Stmt::Loop(body) => lower_loop(ctx, &body.inner),
        Stmt::Unsafe(body) => lower_block_value(ctx, &body.inner),
        Stmt::Break { value, .. } => lower_break(ctx, value.as_ref()),
        Stmt::Continue { .. } => lower_continue(ctx),
        _ => {}
    }
}

/// `while (cond) { body }` — header tests `cond`, body jumps back to header.
fn lower_while(ctx: &mut LowerCtx<'_>, cond: &phx_syntax::ast::ExprNode, body: &Block) {
    let header = ctx.fresh_block();
    let body_id = ctx.fresh_block();
    let exit_slot = ctx.alloc_loop_exit_slot();
    let loop_scope = ctx.scope_depth;
    ctx.loop_body_scope_depths.push(loop_scope);

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
    ctx.loop_body_scope_depths.pop();
    let exit = ctx.fresh_block();
    ctx.pending_loop_exits[exit_slot] = Some(exit);
    ctx.set_current(exit);
}

/// `loop { body }` — body repeats until `break` (or `return`).
fn lower_loop(ctx: &mut LowerCtx<'_>, body: &Block) {
    let header = ctx.fresh_block();
    let exit_slot = ctx.alloc_loop_exit_slot();
    let loop_scope = ctx.scope_depth;
    ctx.loop_body_scope_depths.push(loop_scope);

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
    ctx.loop_body_scope_depths.pop();
    let exit = ctx.fresh_block();
    ctx.pending_loop_exits[exit_slot] = Some(exit);
    ctx.set_current(exit);
}

fn lower_break(ctx: &mut LowerCtx<'_>, expr: Option<&phx_syntax::ast::ExprNode>) {
    if let Some(e) = expr {
        lower_expr(ctx, e);
    }
    emit_scope_drops(ctx, ctx.scope_depth, loop_body_scope_depth(ctx));
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
    emit_scope_drops(ctx, ctx.scope_depth, 0);
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

/// Emits [`IrInst::Return`] after the function body when the tail block does not already return.
pub fn lower_function_return(ctx: &mut LowerCtx<'_>, _body: &Block, return_type: TypeId) {
    if block_ends_with_return(ctx, ctx.current) {
        return;
    }
    emit_scope_drops(ctx, 0, 0);
    ctx.emit(IrInst::Return { ty: return_type });
}

fn block_ends_with_return(ctx: &LowerCtx<'_>, block: u32) -> bool {
    let idx = usize::try_from(block).ok();
    let Some(b) = idx.and_then(|i| ctx.blocks.get(i)) else {
        return false;
    };
    b.insts
        .last()
        .is_some_and(|inst| matches!(inst, IrInst::Return { .. }))
}

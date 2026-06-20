//! Statement and block lowering: bindings, control flow, and function tails.
//!
//! Walks [`Block`](phx_syntax::ast::stmt::Block) items and [`Stmt`](phx_syntax::ast::stmt::Stmt)
//! nodes, emitting CFG edges and expression instructions through [`LowerCtx`](crate::lower::ctx::LowerCtx).
//! Expression operands delegate to [`super::expr`]; scope exit and loop `break`/`return` paths call
//! [`super::drop_glue::emit_scope_drops`] so drop order matches typeck binding depth.
//!
//! ## Invariants
//!
//! - **Scope depth:** each block calls [`LowerCtx::enter_scope`] / [`LowerCtx::exit_scope`]; locals
//!   bind at the current depth and drop when a scope closes or when control exits early (`return`,
//!   `break`, for-in exhaustion).
//! - **Loop labels:** `while`, `loop`, and `for-in` push [`LoopLabels`](crate::lower::ctx::LoopLabels)
//!   and record the body scope in [`LowerCtx::loop_body_scope_depths`] so `break` drops only bindings
//!   created inside the loop body, not outer locals.
//! - **Fall-through:** loop bodies append an unconditional [`IrInst::Jump`] back to the header when
//!   the tail block does not already end in `break`, `continue`, or `return`.
//! - **For-in:** iterator protocol is desugared using [`ForInPlan`](crate::typeck::ForInPlan) from
//!   typeck layout; the binding pattern is synthesized as `Some(binding)` for [`emit_arm_condition`].

use phx_diagnostics::Span;
use phx_syntax::ast::ident::{Ident, TypeName};
use phx_syntax::ast::node_id::AstNodeId;
use phx_syntax::ast::pat::Pattern;
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt, StmtNode};
use phx_syntax::ast::{Node, PatternNode};

use crate::ir::IrInst;
use crate::lower::ctx::{LoopLabels, LowerCtx, prim_kind_byte, unit_ty};
use crate::lower::drop_glue::{emit_scope_drops, loop_body_scope_depth};
use crate::lower::expr::{
    bind_match_pattern, block_ends_with_unconditional_jump, emit_arm_condition, lower_expr,
    lower_expr_with_type,
};
use crate::typeck::{ForInPlan, TypeId};

/// Lowers every item in `block`, leaving the last expression's value on the stack when present.
///
/// Opens a scope for the block, dispatches [`BlockItem::Stmt`] through [`lower_block_stmt`],
/// evaluates trailing [`BlockItem::Expr`] nodes, and ignores imports (already resolved). Closes the
/// scope on exit, which emits drop glue for bindings introduced in the block.
pub fn lower_block_value(ctx: &mut LowerCtx<'_>, block: &Block) {
    ctx.enter_scope();
    for item in &block.items {
        match item {
            BlockItem::Stmt(stmt) => lower_block_stmt(ctx, stmt),
            BlockItem::Expr(expr) => lower_expr(ctx, expr),
            BlockItem::Import(_) => {}
        }
    }
    ctx.exit_scope();
}

/// Dispatches one statement to the appropriate lowering helper.
///
/// Sets the diagnostic site to `stmt.span` before emission. Local bindings lower their initializer
/// then [`IrInst::StoreLocal`] when layout metadata exists; `for-in` consumes the next
/// [`ForInPlan`](crate::typeck::ForInPlan) in layout order.
fn lower_block_stmt(ctx: &mut LowerCtx<'_>, stmt: &StmtNode) {
    ctx.set_site(stmt.span);
    match &stmt.inner {
        Stmt::Const { name, init, .. } | Stmt::Var { name, init, .. } => {
            if let Some(binding) = ctx.layout.binding(name.symbol) {
                lower_expr_with_type(ctx, init, binding.ty);
            } else {
                lower_expr(ctx, init);
            }
            if let Some(binding) = ctx.layout.binding(name.symbol) {
                ctx.emit(
                    name.span,
                    IrInst::StoreLocal {
                        slot: binding.slot,
                        ty: binding.ty,
                        prim_kind: prim_kind_byte(ctx.typed, binding.ty),
                    },
                );
            }
        }
        Stmt::Assign { expr } => {
            lower_expr(ctx, expr);
        }
        Stmt::Expr(expr) => {
            lower_expr(ctx, expr);
        }
        Stmt::Return(expr) => lower_return(ctx, stmt.span, expr.as_ref()),
        Stmt::While { cond, body } => lower_while(ctx, stmt.span, cond, &body.inner),
        Stmt::ForIn { iter, body, .. } => {
            if let Some(plan) = ctx.layout.for_in_plans.get(ctx.for_in_index) {
                lower_for_in(ctx, plan, stmt.span, iter, &body.inner);
                ctx.for_in_index += 1;
            }
        }
        Stmt::Loop(body) => lower_loop(ctx, stmt.span, &body.inner),
        Stmt::Unsafe(body) => lower_block_value(ctx, &body.inner),
        Stmt::Break { value, .. } => lower_break(ctx, stmt.span, value.as_ref()),
        Stmt::Continue { .. } => lower_continue(ctx),
    }
}

/// Lowers `while (cond) { body }`.
///
/// CFG shape: jump to header → [`IrInst::JumpIf`] on `cond` → body block → jump to header → exit
/// block after the loop. Registers loop labels so `break`/`continue` target the exit placeholder
/// and header respectively.
fn lower_while(
    ctx: &mut LowerCtx<'_>,
    stmt_span: Span,
    cond: &phx_syntax::ast::ExprNode,
    body: &Block,
) {
    ctx.set_site(stmt_span);
    let header = ctx.fresh_block();
    let body_id = ctx.fresh_block();
    let exit_slot = ctx.alloc_loop_exit_slot();
    let loop_scope = ctx.scope_depth;
    ctx.loop_body_scope_depths.push(loop_scope);

    ctx.emit_here(IrInst::Jump { target: header });
    ctx.push_loop(LoopLabels {
        exit_slot,
        continue_target: header,
    });

    ctx.set_current(header);
    lower_expr(ctx, cond);
    ctx.emit_here(IrInst::JumpIf {
        then_block: body_id,
        else_block: LowerCtx::loop_exit_target(exit_slot),
    });

    ctx.set_current(body_id);
    lower_block_value(ctx, body);
    if !block_ends_with_unconditional_jump(ctx, ctx.current) {
        ctx.emit_here(IrInst::Jump { target: header });
    }

    ctx.pop_loop();
    ctx.loop_body_scope_depths.pop();
    let exit = ctx.fresh_block();
    ctx.pending_loop_exits[exit_slot] = Some(exit);
    ctx.set_current(exit);
}

/// Lowers `loop { body }`.
///
/// Same loop-label machinery as [`lower_while`], but the header has no condition: every iteration
/// enters the body block directly until `break`, `return`, or another terminating branch.
fn lower_loop(ctx: &mut LowerCtx<'_>, stmt_span: Span, body: &Block) {
    ctx.set_site(stmt_span);
    let header = ctx.fresh_block();
    let exit_slot = ctx.alloc_loop_exit_slot();
    let loop_scope = ctx.scope_depth;
    ctx.loop_body_scope_depths.push(loop_scope);

    ctx.emit_here(IrInst::Jump { target: header });
    ctx.push_loop(LoopLabels {
        exit_slot,
        continue_target: header,
    });

    ctx.set_current(header);
    lower_block_value(ctx, body);
    if !block_ends_with_unconditional_jump(ctx, ctx.current) {
        ctx.emit_here(IrInst::Jump { target: header });
    }

    ctx.pop_loop();
    ctx.loop_body_scope_depths.pop();
    let exit = ctx.fresh_block();
    ctx.pending_loop_exits[exit_slot] = Some(exit);
    ctx.set_current(exit);
}

/// Lowers `break` or `break expr`.
///
/// Evaluates an optional value expression, drops loop-body bindings via
/// [`emit_scope_drops`](crate::lower::drop_glue::emit_scope_drops), then jumps to the innermost
/// loop's exit placeholder. No-op when not inside a loop (typeck rejects this in user code).
fn lower_break(ctx: &mut LowerCtx<'_>, stmt_span: Span, expr: Option<&phx_syntax::ast::ExprNode>) {
    ctx.set_site(stmt_span);
    if let Some(e) = expr {
        lower_expr(ctx, e);
    }
    emit_scope_drops(ctx, ctx.scope_depth, loop_body_scope_depth(ctx));
    if let Some(labels) = ctx.innermost_loop() {
        ctx.emit_here(IrInst::Jump {
            target: LowerCtx::loop_exit_target(labels.exit_slot),
        });
    }
}

/// Lowers `for binding in iter { body }` using the iterator protocol plan from typeck.
///
/// Desugars to: call `IntoIterator`, loop calling `next`, match on `Option` with a synthetic
/// `Some(binding)` pattern, run `body` on `Some`, and jump to exit on `None`. The iterator state
/// lives in `plan.iter_temp_slot`; drop glue runs on the `None` arm before exiting the loop.
fn lower_for_in(
    ctx: &mut LowerCtx<'_>,
    plan: &ForInPlan,
    stmt_span: Span,
    iter: &phx_syntax::ast::ExprNode,
    body: &Block,
) {
    ctx.set_site(stmt_span);
    ctx.enter_scope();

    lower_expr(ctx, iter);
    ctx.emit_here(IrInst::Call {
        callee: plan.into_iter_fn,
        ret: plan.iter_state_ty,
    });
    ctx.emit_here(IrInst::StoreLocal {
        slot: plan.iter_temp_slot,
        ty: plan.iter_state_ty,
        prim_kind: prim_kind_byte(ctx.typed, plan.iter_state_ty),
    });

    let header = ctx.fresh_block();
    let body_id = ctx.fresh_block();
    let else_id = ctx.fresh_block();
    let exit_slot = ctx.alloc_loop_exit_slot();
    let loop_scope = ctx.scope_depth;
    ctx.loop_body_scope_depths.push(loop_scope);

    ctx.emit_here(IrInst::Jump { target: header });
    ctx.push_loop(LoopLabels {
        exit_slot,
        continue_target: header,
    });

    ctx.set_current(header);
    ctx.emit_here(IrInst::AddressOfLocal {
        slot: plan.iter_temp_slot,
    });
    ctx.emit_here(IrInst::Call {
        callee: plan.next_fn,
        ret: plan.option_ty,
    });
    ctx.emit_here(IrInst::StoreLocal {
        slot: plan.option_match_temp,
        ty: plan.option_ty,
        prim_kind: prim_kind_byte(ctx.typed, plan.option_ty),
    });
    let some_pat = some_binding_pattern(plan.binding, plan.some_variant, plan.stmt_span);
    emit_arm_condition(
        ctx,
        &some_pat.inner,
        plan.option_match_temp,
        plan.option_ty,
        body_id,
        else_id,
    );

    ctx.set_current(body_id);
    bind_match_pattern(
        ctx,
        &some_pat.inner,
        plan.option_match_temp,
        plan.option_ty,
        false,
    );
    lower_block_value(ctx, body);
    if !block_ends_with_unconditional_jump(ctx, ctx.current) {
        ctx.emit_here(IrInst::Jump { target: header });
    }

    ctx.set_current(else_id);
    emit_scope_drops(ctx, ctx.scope_depth, loop_body_scope_depth(ctx));
    ctx.emit_here(IrInst::Jump {
        target: LowerCtx::loop_exit_target(exit_slot),
    });

    ctx.pop_loop();
    ctx.loop_body_scope_depths.pop();
    let exit = ctx.fresh_block();
    ctx.pending_loop_exits[exit_slot] = Some(exit);
    ctx.set_current(exit);
    ctx.exit_scope();
}

/// Builds a synthetic `Some(binding)` pattern for for-in `next` dispatch.
///
/// Uses `some_variant` from the typed `Option` enum and a dummy AST node id — only the shape is
/// needed for [`emit_arm_condition`] and [`bind_match_pattern`].
fn some_binding_pattern(
    binding: phx_syntax::Symbol,
    some_variant: phx_syntax::Symbol,
    span: Span,
) -> PatternNode {
    let dummy_id = AstNodeId::from_raw(0);
    Node::new(
        Pattern::Tuple {
            name: TypeName {
                symbol: some_variant,
                span,
                id: dummy_id,
            },
            patterns: vec![Node::new(
                Pattern::Ident(Ident {
                    symbol: binding,
                    span,
                    id: dummy_id,
                }),
                span,
                dummy_id,
            )],
        },
        span,
        dummy_id,
    )
}

/// Lowers `continue` by jumping to the innermost loop's continue target (header or condition).
fn lower_continue(ctx: &mut LowerCtx<'_>) {
    if let Some(labels) = ctx.innermost_loop() {
        ctx.emit_here(IrInst::Jump {
            target: labels.continue_target,
        });
    }
}

/// Lowers `return` or `return expr`.
///
/// Drops all bindings from the current scope depth down to the function root (`to_depth` 0), then
/// emits [`IrInst::Return`] with the function return type or unit when the expression is omitted.
fn lower_return(ctx: &mut LowerCtx<'_>, stmt_span: Span, expr: Option<&phx_syntax::ast::ExprNode>) {
    ctx.set_site(stmt_span);
    emit_scope_drops(ctx, ctx.scope_depth, 0);
    if let Some(e) = expr {
        lower_expr(ctx, e);
        ctx.emit_here(IrInst::Return {
            ty: ctx.layout.return_type,
        });
    } else {
        ctx.emit_here(IrInst::Return {
            ty: unit_ty(ctx.typed),
        });
    }
}

/// Appends a fall-through [`IrInst::Return`] when the function tail block lacks one.
///
/// Called by [`super::func::lower_one_function`] after the body block is lowered. Skips emission
/// when [`block_ends_with_return`] is true (e.g. every path ends in `return`). Otherwise drops
/// function-root bindings and returns with `return_type`.
pub fn lower_function_return(
    ctx: &mut LowerCtx<'_>,
    body: &phx_syntax::ast::BlockNode,
    return_type: TypeId,
) {
    if block_ends_with_return(ctx, ctx.current) {
        return;
    }
    ctx.set_site(body.span);
    emit_scope_drops(ctx, 0, 0);
    ctx.emit_here(IrInst::Return { ty: return_type });
}

/// True when `block`'s last instruction is [`IrInst::Return`].
fn block_ends_with_return(ctx: &LowerCtx<'_>, block: u32) -> bool {
    let idx = usize::try_from(block).ok();
    let Some(b) = idx.and_then(|i| ctx.blocks.get(i)) else {
        return false;
    };
    b.insts
        .last()
        .is_some_and(|spanned| matches!(&spanned.inst, IrInst::Return { .. }))
}

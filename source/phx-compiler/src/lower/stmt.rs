//! Lower statements and blocks to IR control flow.

use phx_syntax::ast::expr::Expr;
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};

use crate::ir::IrInst;
use crate::lower::ctx::{LowerCtx, unit_ty};
use crate::lower::expr::{lower_assign_expr, lower_expr};
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
        Stmt::While { cond, body } => {
            lower_expr(ctx, cond);
            lower_block_value(ctx, &body.inner);
        }
        Stmt::Loop(body) => lower_block_value(ctx, &body.inner),
        Stmt::Given {
            scrutinee, body, ..
        } => {
            lower_expr(ctx, scrutinee);
            lower_block_value(ctx, &body.inner);
        }
        Stmt::Unsafe(body) => lower_block_value(ctx, &body.inner),
        Stmt::Break(_) | Stmt::Continue => {}
        _ => {}
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

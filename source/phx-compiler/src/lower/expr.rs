//! Lower expressions to IR instructions (stack-oriented).

use phx_syntax::ast::expr::{BinOp, Expr, ExprNode, PostfixOp};
use phx_syntax::ast::ident::{Ident, Path, PathSegment};
use phx_syntax::ast::lit::Literal;
use phx_syntax::ast::pat::MatchArm;
use phx_syntax::ast::stmt::BlockNode;

use crate::ir::{IrBinOp, IrInst};
use crate::lower::ctx::{LowerCtx, const_index_for_literal, lookup_resolution, slot_for_symbol};
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
        Expr::EnumCtor {
            inner: Some(inner), ..
        } => lower_expr(ctx, inner),
        Expr::EnumCtor { inner: None, .. } => {}
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
        BinOp::Or
        | BinOp::And
        | BinOp::Eq
        | BinOp::Lt
        | BinOp::Add
        | BinOp::Sub
        | BinOp::Mul
        | BinOp::Div => {
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
}

fn lower_assign_target(ctx: &mut LowerCtx<'_>, target: &Expr) {
    match target {
        Expr::Ident(_) => {}
        Expr::Postfix { base, ops } if ops.len() == 1 => {
            if let PostfixOp::Field(_) = &ops[0] {
                lower_expr(ctx, base);
            }
        }
        Expr::Literal(_) | Expr::Path(_) | Expr::EnumCtor { .. } => {}
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
    ctx.emit(IrInst::Jump { target: merge_id });

    ctx.set_current(else_id);
    if else_ifs.is_empty() {
        if let Some(else_b) = else_block {
            lower_block_expr(ctx, else_b);
        }
    } else {
        lower_else_if_chain(ctx, else_ifs, else_block);
    }
    ctx.emit(IrInst::Jump { target: merge_id });

    ctx.set_current(merge_id);
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
    ctx.emit(IrInst::Jump { target: merge_id });

    ctx.set_current(else_id);
    if rest.is_empty() {
        if let Some(else_b) = final_else {
            lower_block_expr(ctx, else_b);
        }
    } else {
        lower_else_if_chain(ctx, rest, final_else);
    }
    ctx.emit(IrInst::Jump { target: merge_id });

    ctx.set_current(merge_id);
}

fn lower_match(ctx: &mut LowerCtx<'_>, scrutinee: &ExprNode, arms: &[MatchArm]) {
    lower_expr(ctx, scrutinee);
    for arm in arms {
        if let Some(guard) = &arm.guard {
            lower_expr(ctx, guard);
        }
        lower_expr(ctx, &arm.body);
    }
}

fn lower_block_expr(ctx: &mut LowerCtx<'_>, block: &BlockNode) {
    crate::lower::stmt::lower_block_value(ctx, &block.inner);
}

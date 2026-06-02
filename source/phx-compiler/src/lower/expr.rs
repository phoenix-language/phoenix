//! Lower expressions to IR instructions (stack-oriented).

use phx_syntax::Symbol;
use phx_syntax::ast::expr::{BinOp, Expr, ExprNode, PostfixOp, StructFieldInit};
use phx_syntax::ast::ident::{Ident, Path, PathSegment, TypeName};
use phx_syntax::ast::lit::Literal;
use phx_syntax::ast::pat::{MatchArm, Pattern};
use phx_syntax::ast::stmt::BlockNode;

use crate::ir::{IrBinOp, IrInst};
use crate::lower::ctx::{
    LowerCtx, bool_ty, lookup_resolution, named_def_for_ty,
    slot_for_symbol, struct_def_by_name, unit_ty,
};
use crate::resolver::DefId;
use crate::ir::IrConst;
use crate::typeck::{
    LocalSlot, TypeId, Ty, VariantKind, primitive_byte_size, primitive_kind_for_type,
    primitive_load_signed,
};
use phx_bytecode::{PrimitiveKind, ScalarValue};
use phx_syntax::token::IntegerSuffix;

/// Lowers `expr` so its value is on the implicit stack.
pub fn lower_expr(ctx: &mut LowerCtx<'_>, expr: &ExprNode) {
    let ty = ctx.expr_ty();
    lower_expr_inner(ctx, &expr.inner, ty);
}

/// Lowers `expr` and returns its typeck-assigned type.
fn lower_expr_typed(ctx: &mut LowerCtx<'_>, expr: &ExprNode) -> TypeId {
    let ty = ctx.expr_ty();
    lower_expr_inner(ctx, &expr.inner, ty);
    ty
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
            let arity = u32::try_from(items.len()).unwrap_or(u32::MAX);
            ctx.emit(IrInst::MakeTuple { arity });
        }
        Expr::Array(items) => {
            for item in items {
                lower_expr(ctx, item);
            }
            let len = u32::try_from(items.len()).unwrap_or(u32::MAX);
            ctx.emit(IrInst::MakeArray { len });
        }
        Expr::Unary { op, operand } => {
            use phx_syntax::ast::expr::UnaryOp;
            lower_expr(ctx, operand);
            match op {
                UnaryOp::Neg => {
                    ctx.emit(IrInst::Neg { result: result_ty });
                }
                UnaryOp::Not => {
                    ctx.emit(IrInst::Not { result: result_ty });
                }
                UnaryOp::BitNot => {
                    ctx.emit(IrInst::BitNot { result: result_ty });
                }
                UnaryOp::Deref => {
                    if let Some(kind) = primitive_kind_for_type(&ctx.typed.types, result_ty) {
                        ctx.emit(IrInst::PtrLoad {
                            byte_size: primitive_byte_size(kind),
                            signed: primitive_load_signed(kind),
                            result: result_ty,
                        });
                    }
                }
                _ => {}
            }
        }
        Expr::Binary { op, left, right } => {
            lower_binary(ctx, *op, left, right, result_ty);
        }
        Expr::Assign { target, value, .. } => {
            lower_assign_expr(ctx, target, value);
        }
        Expr::Cast { expr, .. } => {
            let from_id = crate::typeck::ExprId::from_raw(ctx.next_expr);
            let from_ty = ctx
                .typed
                .expr_types
                .get(&from_id)
                .copied()
                .unwrap_or(result_ty);
            lower_expr(ctx, expr);
            if from_ty != result_ty {
                if let (Some(from_k), Some(to_k)) = (
                    primitive_kind_for_type(&ctx.typed.types, from_ty),
                    primitive_kind_for_type(&ctx.typed.types, result_ty),
                ) {
                    ctx.emit(IrInst::Cast {
                        from_kind: from_k.as_u8(),
                        to_kind: to_k.as_u8(),
                    });
                }
            }
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
        Expr::StructLit { name, fields, .. } => {
            if let Some(def) = struct_def_by_name(&ctx.typed.resolved, name.symbol) {
                if ctx.typed.layout.structs.contains_key(&def) {
                    let type_id = ctx.typed.layout.type_id(def).unwrap_or(0);
                    let mut field_count = 0u32;
                    for field in fields {
                        if let StructFieldInit::Field { value, .. } = field {
                            lower_expr(ctx, value);
                            field_count = field_count.saturating_add(1);
                        }
                    }
                    ctx.emit(IrInst::MakeStruct {
                        type_id,
                        field_count,
                    });
                }
            }
        }
        Expr::Unsafe(block) => lower_block_expr(ctx, block),
        _ => {}
    }
}

fn intern_literal(ctx: &mut LowerCtx<'_>, lit: &Literal, ty: TypeId) -> Option<u32> {
    match lit {
        Literal::Int(i) => {
            let Some(to) = primitive_kind_for_type(&ctx.typed.types, ty) else {
                return None;
            };
            let from = if i.suffix == IntegerSuffix::Unsigned {
                PrimitiveKind::U32
            } else {
                PrimitiveKind::S32
            };
            let raw = i64::try_from(i.value).unwrap_or_else(|_| {
                if i.value < 0 {
                    i64::MIN
                } else {
                    i64::MAX
                }
            });
            let stored = PrimitiveKind::apply_cast(ScalarValue::Int(raw), from, to);
            Some(match stored {
                ScalarValue::Int(n) => ctx.intern_const(IrConst::Int(n)),
                ScalarValue::Float(f) => ctx.intern_const(IrConst::Float(f)),
            })
        }
        Literal::Float(f) => Some(ctx.intern_const(IrConst::Float(f.value))),
        Literal::Bool(b) => Some(ctx.intern_const(IrConst::Bool(*b))),
        Literal::ByteChar(_) | Literal::ByteString(_) => None,
        _ => None,
    }
}

fn lower_literal(ctx: &mut LowerCtx<'_>, lit: &Literal, ty: TypeId) {
    let Some(index) = intern_literal(ctx, lit, ty) else {
        return;
    };
    ctx.emit(IrInst::Const { index, ty });
    if matches!(lit, Literal::Float(_))
        && matches!(
            ctx.typed.types.get(ty),
            Ty::Primitive(phx_syntax::token::Keyword::F32)
        )
    {
        ctx.emit(IrInst::Cast {
            from_kind: PrimitiveKind::F64.as_u8(),
            to_kind: PrimitiveKind::F32.as_u8(),
        });
    }
}

fn lower_ident(ctx: &mut LowerCtx<'_>, ident: Ident, ty: TypeId) {
    let symbol = ident.symbol;
    if let Some(slot) = slot_for_symbol(ctx.layout, symbol) {
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
            lower_expr(ctx, left);
            lower_expr(ctx, right);
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Ge,
                result: result_ty,
            });
        }
        BinOp::Le => {
            lower_expr(ctx, left);
            lower_expr(ctx, right);
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Le,
                result: result_ty,
            });
        }
        BinOp::Ne => {
            lower_expr(ctx, left);
            lower_expr(ctx, right);
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Ne,
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
            if let Some(ir_op) = binop_to_ir(op) {
                ctx.emit(IrInst::BinOp {
                    op: ir_op,
                    result: result_ty,
                });
            }
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
        BinOp::Ne => Some(IrBinOp::Ne),
        BinOp::Le => Some(IrBinOp::Le),
        BinOp::Ge => Some(IrBinOp::Ge),
        BinOp::Mod => Some(IrBinOp::Mod),
        BinOp::Pow => Some(IrBinOp::Pow),
        BinOp::BitOr => Some(IrBinOp::BitOr),
        BinOp::BitXor => Some(IrBinOp::BitXor),
        BinOp::BitAnd => Some(IrBinOp::BitAnd),
        BinOp::Shl => Some(IrBinOp::Shl),
        BinOp::Shr => Some(IrBinOp::Shr),
        BinOp::Or | BinOp::And | BinOp::Gt => None,
        _ => None,
    }
}

pub(crate) fn lower_assign_expr(ctx: &mut LowerCtx<'_>, target: &ExprNode, value: &ExprNode) {
    lower_assign_target(ctx, &target.inner);
    lower_expr(ctx, value);
    match &target.inner {
        Expr::Ident(ident) => {
            if let Some(binding) = ctx.layout.binding(ident.symbol) {
                ctx.emit(IrInst::StoreLocal {
                    slot: binding.slot,
                    ty: binding.ty,
                });
            }
        }
        Expr::Postfix { base, ops } if ops.len() == 1 => {
            if let PostfixOp::Field(field) = &ops[0] {
                if let Some(def) = struct_def_from_base(ctx, base) {
                    let type_id = ctx.typed.layout.type_id(def).unwrap_or(0);
                    let field_index = ctx
                        .typed
                        .layout
                        .struct_field_index(def, field.symbol)
                        .unwrap_or(0);
                    ctx.emit(IrInst::SetField {
                        type_id,
                        field_index,
                    });
                    store_base_local(ctx, base);
                }
            }
        }
        _ => {}
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
    let mut receiver_ty = lower_expr_typed(ctx, base);
    for op in ops {
        match op {
            PostfixOp::Field(field) => {
                if let Some(def) = named_def_for_ty(ctx.typed, receiver_ty) {
                    let type_id = ctx.typed.layout.type_id(def).unwrap_or(0);
                    let field_index = ctx
                        .typed
                        .layout
                        .struct_field_index(def, field.symbol)
                        .unwrap_or(0);
                    ctx.emit(IrInst::GetField {
                        type_id,
                        field_index,
                        result: result_ty,
                    });
                }
            }
            PostfixOp::Method { name, args, .. } => {
                if let Some(type_def) = named_def_for_ty(ctx.typed, receiver_ty) {
                    let callee = ctx
                        .typed
                        .layout
                        .inherent_methods
                        .get(&(type_def, name.symbol))
                        .copied()
                        .or_else(|| find_trait_method(ctx, type_def, name.symbol));
                    if let Some(callee) = callee {
                        for arg in args {
                            lower_expr(ctx, arg);
                        }
                        ctx.emit(IrInst::Call {
                            callee,
                            ret: result_ty,
                        });
                    }
                }
            }
            PostfixOp::Call(args) => {
                if let Some(variant_def) = resolve_variant_ctor(ctx, base) {
                    if let Some(meta) = ctx.typed.layout.variants.get(&variant_def) {
                        for arg in args {
                            lower_expr(ctx, arg);
                        }
                        let type_id = ctx.typed.layout.type_id(meta.enum_def).unwrap_or(0);
                        let payload_count = u32::try_from(args.len()).unwrap_or(u32::MAX);
                        ctx.emit(IrInst::MakeEnum {
                            type_id,
                            variant_tag: meta.tag,
                            payload_count,
                        });
                        receiver_ty = result_ty;
                    }
                } else {
                    let callee = resolve_call_callee(ctx, base);
                    for arg in args {
                        lower_expr(ctx, arg);
                    }
                    ctx.emit(IrInst::Call {
                        callee,
                        ret: result_ty,
                    });
                }
            }
            PostfixOp::Index(idx) => {
                lower_expr(ctx, idx);
                ctx.emit(IrInst::Index { result: result_ty });
            }
            PostfixOp::Try => {}
            _ => {}
        }
    }
}

fn find_trait_method(ctx: &LowerCtx<'_>, type_def: DefId, method: Symbol) -> Option<DefId> {
    let mut matches: Vec<DefId> = ctx
        .typed
        .layout
        .trait_methods
        .iter()
        .filter(|((t, _, m), _)| *t == type_def && *m == method)
        .map(|(_, f)| *f)
        .collect();
    matches.sort_by_key(|d| d.index());
    matches.dedup();
    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    }
}

fn resolve_variant_ctor(ctx: &LowerCtx<'_>, base: &ExprNode) -> Option<DefId> {
    let symbol = path_or_ident_symbol(&base.inner)?;
    let def = lookup_resolution(&ctx.typed.resolved, base.span, symbol)?;
    if ctx.typed.layout.variants.contains_key(&def) {
        Some(def)
    } else {
        None
    }
}

fn resolve_call_callee(ctx: &LowerCtx<'_>, base: &ExprNode) -> DefId {
    if let Some(symbol) = path_or_ident_symbol(&base.inner) {
        lookup_resolution(&ctx.typed.resolved, base.span, symbol).unwrap_or(DefId::from_raw(0))
    } else {
        DefId::from_raw(0)
    }
}

fn path_or_ident_symbol(expr: &Expr) -> Option<Symbol> {
    match expr {
        Expr::Ident(ident) => Some(ident.symbol),
        Expr::Path(path) if path.segments.len() == 1 => match path.segments[0] {
            PathSegment::Ident(ident) => Some(ident.symbol),
            PathSegment::Type(TypeName { symbol, .. }) => Some(symbol),
            _ => None,
        },
        _ => None,
    }
}

fn struct_def_from_base(ctx: &LowerCtx<'_>, base: &ExprNode) -> Option<DefId> {
    if let Expr::Ident(ident) = &base.inner {
        if let Some(binding) = ctx.layout.binding(ident.symbol) {
            if let Some(def) = named_def_for_ty(ctx.typed, binding.ty) {
                if ctx.typed.layout.structs.contains_key(&def) {
                    return Some(def);
                }
            }
        }
    }
    None
}

fn store_base_local(ctx: &mut LowerCtx<'_>, base: &ExprNode) {
    if let Expr::Ident(ident) = &base.inner
        && let Some(binding) = ctx.layout.binding(ident.symbol)
    {
        ctx.emit(IrInst::StoreLocal {
            slot: binding.slot,
            ty: binding.ty,
        });
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

    ctx.set_current(merge_id);
}

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
            if let Some((type_id, tag)) = find_enum_variant_by_name(ctx, ident.symbol) {
                ctx.emit(IrInst::LoadLocal {
                    slot: temp,
                    ty: temp_ty,
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
            });
            if let Some(index) = intern_literal(ctx, lit, temp_ty) {
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
        Pattern::Struct { .. } => {
            ctx.emit(IrInst::Jump { target: body_id });
        }
        Pattern::Tuple { name, .. } => {
            if let Some((type_id, tag)) = find_enum_variant_by_name(ctx, name.symbol) {
                ctx.emit(IrInst::LoadLocal {
                    slot: temp,
                    ty: temp_ty,
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
        _ => {
            ctx.emit(IrInst::Jump { target: fail_id });
        }
    }
}

fn find_enum_variant_by_name(ctx: &LowerCtx<'_>, name: Symbol) -> Option<(u32, u32)> {
    for (&enum_def, el) in &ctx.typed.layout.enums {
        for v in &el.variants {
            if v.name == name {
                let type_id = ctx.typed.layout.type_id(enum_def)?;
                return Some((type_id, v.tag));
            }
        }
    }
    None
}

pub(crate) fn bind_match_pattern(
    ctx: &mut LowerCtx<'_>,
    pat: &Pattern,
    temp: LocalSlot,
    temp_ty: TypeId,
    value_on_stack: bool,
) {
    match pat {
        Pattern::Ident(ident) => {
            if find_enum_variant_by_name(ctx, ident.symbol).is_some() {
                return;
            }
            if let Some(binding) = ctx.layout.binding(ident.symbol) {
                if !value_on_stack {
                    ctx.emit(IrInst::LoadLocal {
                        slot: temp,
                        ty: temp_ty,
                    });
                }
                ctx.emit(IrInst::StoreLocal {
                    slot: binding.slot,
                    ty: binding.ty,
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
                        .struct_field_index(def, field.name.symbol)
                        .unwrap_or(0);
                    ctx.emit(IrInst::LoadLocal {
                        slot: temp,
                        ty: temp_ty,
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
                        });
                    }
                }
            }
        }
        Pattern::Tuple { name, patterns } => {
            for el in ctx.typed.layout.enums.values() {
                if let Some(variant) = el.variants.iter().find(|v| v.name == name.symbol) {
                    let type_id = ctx.typed.layout.type_id(el.enum_def).unwrap_or(0);
                    if let VariantKind::Tuple(payload) = &variant.kind {
                        for (i, p) in patterns.iter().enumerate() {
                            ctx.emit(IrInst::LoadLocal {
                                slot: temp,
                                ty: temp_ty,
                            });
                            let result_ty = payload
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
                    break;
                }
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) => {}
        _ => {}
    }
}

fn field_result_ty(ctx: &LowerCtx<'_>, struct_def: DefId, field: Symbol) -> TypeId {
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
        .unwrap_or_else(|| unit_ty(ctx.typed))
}

fn lower_block_expr(ctx: &mut LowerCtx<'_>, block: &BlockNode) {
    crate::lower::stmt::lower_block_value(ctx, &block.inner);
}

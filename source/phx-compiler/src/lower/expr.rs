//! Lower expressions to IR instructions (stack-oriented).

use phx_syntax::Symbol;
use phx_syntax::ast::expr::{
    BinOp, Expr, ExprNode, IfCondition, PostfixOp, StructFieldInit, UnaryOp,
};
use phx_syntax::ast::ident::{Ident, Path, PathSegment};
use phx_syntax::ast::lit::Literal;
use phx_syntax::ast::pat::{MatchArm, Pattern};
use phx_syntax::ast::stmt::BlockNode;

use crate::TypedProgram;
use crate::ir::IrConst;
use crate::ir::{IrBinOp, IrInst};
use crate::lower::ctx::{
    LowerCtx, bool_ty, fn_ptr_target, lookup_resolution, prim_kind_byte, slot_for_symbol,
    struct_def_by_name, unit_ty,
};
use crate::resolver::{DefId, DefKind};
use crate::typeck::{
    BindingKind, ExprId, FunctionLayout, IntrinsicSite, LocalSlot, PrimitiveMethodSite,
    TryFailureMode, TrySiteMeta, Ty, TypeId, VariantKind, primitive_kind_for_type,
    primitive_load_signed,
};
use phx_bytecode::{PrimitiveKind, SLOT_KIND_AGG, ScalarValue};
use phx_syntax::token::IntegerSuffix;

/// Lowers `expr` so its value is on the implicit stack.
pub fn lower_expr(ctx: &mut LowerCtx<'_>, expr: &ExprNode) {
    let ty = ctx.expr_ty();
    let expr_id = ExprId::from_raw(ctx.next_expr - 1);
    lower_expr_inner(ctx, &expr.inner, ty, expr_id);
}

/// Lowers `expr` with an explicit type (generic impl templates without body `expr_types`).
pub fn lower_expr_with_type(ctx: &mut LowerCtx<'_>, expr: &ExprNode, ty: TypeId) {
    let _ = ctx.expr_ty();
    let expr_id = ExprId::from_raw(ctx.next_expr - 1);
    lower_expr_inner(ctx, &expr.inner, ty, expr_id);
}

/// Lowers `expr` and returns its typeck-assigned type.
fn lower_expr_typed(ctx: &mut LowerCtx<'_>, expr: &ExprNode) -> TypeId {
    let ty = ctx.expr_ty();
    let expr_id = ExprId::from_raw(ctx.next_expr - 1);
    lower_expr_inner(ctx, &expr.inner, ty, expr_id);
    ty
}

#[allow(clippy::too_many_lines)]
fn lower_expr_inner(ctx: &mut LowerCtx<'_>, expr: &Expr, result_ty: TypeId, expr_id: ExprId) {
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
            if matches!(op, UnaryOp::Ref | UnaryOp::RefMut) {
                if let Expr::Ident(ident) = &operand.inner {
                    if let Some(slot) = slot_for_symbol(ctx.layout, ident.symbol) {
                        ctx.emit(IrInst::AddressOfLocal { slot });
                    }
                }
                return;
            }
            lower_expr(ctx, operand);
            match op {
                UnaryOp::Neg => {
                    ctx.emit(IrInst::Neg {
                        result: result_ty,
                        prim_kind: prim_kind_byte(ctx.typed, result_ty),
                    });
                }
                UnaryOp::Not => {
                    ctx.emit(IrInst::Not {
                        result: result_ty,
                        prim_kind: prim_kind_byte(ctx.typed, result_ty),
                    });
                }
                UnaryOp::BitNot => {
                    ctx.emit(IrInst::BitNot {
                        result: result_ty,
                        prim_kind: prim_kind_byte(ctx.typed, result_ty),
                    });
                }
                UnaryOp::Deref => {
                    if let Some(kind) = primitive_kind_for_type(&ctx.typed.types, result_ty) {
                        ctx.emit(IrInst::PtrLoad {
                            prim_kind: kind.as_u8(),
                            signed: primitive_load_signed(kind),
                            result: result_ty,
                        });
                    } else if aggregate_deref_target(&ctx.typed.types, result_ty) {
                        ctx.emit(IrInst::LoadAggViaLocalPtr);
                    }
                }
                UnaryOp::Ref | UnaryOp::RefMut => {
                    // `AddressOfLocal` emitted above; operand already consumed.
                }
                #[allow(unreachable_patterns)]
                _ => {
                    // Reserved for future `UnaryOp` variants (`#[non_exhaustive]`).
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
            let from_id = crate::typeck::ExprId::from_raw(ctx.next_expr);
            let from_ty = ctx
                .typed
                .expr_types
                .get(&from_id)
                .copied()
                .unwrap_or(result_ty);
            if matches!(ctx.typed.types.get(result_ty), Ty::Str) {
                if let Some(bytes) = utf8_bytes_for_str_cast(ctx, &expr.inner) {
                    let idx = ctx.intern_const(IrConst::Bytes(bytes));
                    ctx.next_expr += 1;
                    ctx.emit(IrInst::MakeStr { pool_index: idx });
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
                        ctx.emit(IrInst::MakeSlice { elem_kind });
                    }
                } else if matches!(ctx.typed.types.get(from_ty), Ty::Str) {
                    if let Ty::Slice(inner) = ctx.typed.types.get(result_ty) {
                        if matches!(
                            ctx.typed.types.get(*inner),
                            Ty::Primitive(phx_syntax::token::Keyword::U8)
                        ) {
                            ctx.emit(IrInst::StrAsSlice);
                        }
                    }
                } else if let (Some(from_k), Some(to_k)) = (
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
                if ctx.typed.layout.struct_layout(type_def, &args).is_some() {
                    let type_id = ctx
                        .typed
                        .layout
                        .type_id_for_named(type_def, &args)
                        .unwrap_or(0);
                    let struct_fields = ctx
                        .typed
                        .layout
                        .struct_layout(type_def, &args)
                        .map(|s| s.fields.as_slice());
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
                    ctx.emit(IrInst::MakeStruct {
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
                if let VariantKind::Struct(payload) = payload {
                    let type_id = ctx
                        .typed
                        .layout
                        .type_id_for_named(enum_def, &args)
                        .unwrap_or(0);
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
                    ctx.emit(IrInst::MakeEnum {
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
        #[allow(unreachable_patterns)]
        _ => {
            // Reserved for future `Expr` variants (`#[non_exhaustive]`).
        }
    }
}

fn intern_literal(ctx: &mut LowerCtx<'_>, lit: &Literal, ty: TypeId) -> Option<u32> {
    match lit {
        Literal::Int(i) => {
            let from = if i.suffix == IntegerSuffix::Unsigned {
                PrimitiveKind::U32
            } else {
                PrimitiveKind::S32
            };
            let to = primitive_kind_for_type(&ctx.typed.types, ty).unwrap_or(from);
            let raw = i.value;
            #[allow(clippy::cast_possible_truncation)]
            let stored = PrimitiveKind::apply_cast(ScalarValue::I32(raw as i32), from, to);
            let (value, kind) = scalar_to_ir_const(stored, to);
            Some(ctx.intern_const(IrConst::Int(value, kind)))
        }
        Literal::Float(f) => {
            let kind = primitive_kind_for_type(&ctx.typed.types, ty).unwrap_or(PrimitiveKind::F64);
            Some(ctx.intern_const(IrConst::Float(f.value, kind)))
        }
        Literal::Bool(b) => Some(ctx.intern_const(IrConst::Bool(*b))),
        Literal::ByteChar(c) => {
            let u8_ty = primitive_kind_for_type(&ctx.typed.types, ty).unwrap_or(PrimitiveKind::U8);
            Some(ctx.intern_const(IrConst::Int(i128::from(*c), u8_ty)))
        }
        Literal::ByteString(b) => Some(ctx.intern_const(IrConst::Bytes(b.clone()))),
        Literal::String(s) => Some(ctx.intern_const(IrConst::Bytes(s.clone().into_bytes()))),
        _ => None,
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]
fn scalar_to_ir_const(v: ScalarValue, kind: PrimitiveKind) -> (i128, PrimitiveKind) {
    let n = match v {
        ScalarValue::I8(x) => i128::from(x),
        ScalarValue::I16(x) => i128::from(x),
        ScalarValue::I32(x) => i128::from(x),
        ScalarValue::I64(x) => i128::from(x),
        ScalarValue::I128(x) => x,
        ScalarValue::U8(x) => i128::from(x),
        ScalarValue::U16(x) => i128::from(x),
        ScalarValue::U32(x) => i128::from(x),
        ScalarValue::U64(x) => i128::from(x),
        ScalarValue::U128(x) => x.cast_signed(),
        ScalarValue::F32(x) => return (i128::from(x as i32), kind),
        ScalarValue::F64(x) => return (x as i128, kind),
        ScalarValue::Bool(b) => i128::from(b),
        ScalarValue::Ptr(p) => i128::from(p),
    };
    (n, kind)
}

/// UTF-8 bytes for `[u8; N] as str` / literal casts that lower to rodata [`IrInst::MakeStr`].
fn utf8_bytes_for_str_cast(ctx: &LowerCtx<'_>, expr: &Expr) -> Option<Vec<u8>> {
    match expr {
        Expr::Literal(Literal::ByteString(b)) => {
            if std::str::from_utf8(b).is_ok() {
                Some(b.clone())
            } else {
                None
            }
        }
        Expr::Literal(Literal::String(s)) => Some(s.clone().into_bytes()),
        Expr::Ident(ident) => ctx.layout.binding(ident.symbol).and_then(|b| {
            if b.kind == BindingKind::Const {
                b.utf8_rodata.clone()
            } else {
                None
            }
        }),
        _ => None,
    }
}

fn literal_prim_kind(ctx: &LowerCtx<'_>, lit: &Literal, ty: TypeId) -> u8 {
    if let Some(k) = primitive_kind_for_type(&ctx.typed.types, ty) {
        return k.as_u8();
    }
    match lit {
        Literal::Int(i) => {
            if i.suffix == IntegerSuffix::Unsigned {
                PrimitiveKind::U32.as_u8()
            } else {
                PrimitiveKind::S32.as_u8()
            }
        }
        Literal::Float(_) => PrimitiveKind::F64.as_u8(),
        Literal::Bool(_) => PrimitiveKind::Bool.as_u8(),
        Literal::ByteChar(_) => PrimitiveKind::U8.as_u8(),
        _ => SLOT_KIND_AGG,
    }
}

fn lower_literal(ctx: &mut LowerCtx<'_>, lit: &Literal, ty: TypeId) {
    if let Literal::String(s) = lit {
        let idx = ctx.intern_const(IrConst::Bytes(s.clone().into_bytes()));
        ctx.emit(IrInst::MakeStr { pool_index: idx });
        return;
    }
    if let Literal::ByteString(b) = lit {
        let elem_ty = match ctx.typed.types.get(ty) {
            Ty::Array { elem, .. } => *elem,
            _ => ty,
        };
        for &byte in b {
            let idx = ctx.intern_const(IrConst::Int(i128::from(byte), PrimitiveKind::U8));
            ctx.emit(IrInst::Const {
                index: idx,
                ty: elem_ty,
                prim_kind: PrimitiveKind::U8.as_u8(),
            });
        }
        let len = u32::try_from(b.len()).unwrap_or(0);
        ctx.emit(IrInst::MakeArray { len });
        return;
    }
    let Some(index) = intern_literal(ctx, lit, ty) else {
        return;
    };
    ctx.emit(IrInst::Const {
        index,
        ty,
        prim_kind: literal_prim_kind(ctx, lit, ty),
    });
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
    if let Some(binding) = ctx.layout.binding(symbol) {
        ctx.emit(IrInst::LoadLocal {
            slot: binding.slot,
            ty,
            prim_kind: prim_kind_byte(ctx.typed, binding.ty),
        });
        return;
    }
    if let Some(def) = lookup_resolution(&ctx.typed.resolved, ctx.module, ident.id) {
        if let Some((target_kind, target_id)) = fn_ptr_target(ctx.typed, def) {
            ctx.emit(IrInst::MakeFnPtr {
                target_kind,
                target_id,
                ty,
            });
        }
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
                prim_kind: prim_kind_byte(ctx.typed, result_ty),
            });
        }
        BinOp::Ge => {
            lower_expr(ctx, left);
            lower_expr(ctx, right);
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Ge,
                result: result_ty,
                prim_kind: prim_kind_byte(ctx.typed, result_ty),
            });
        }
        BinOp::Le => {
            lower_expr(ctx, left);
            lower_expr(ctx, right);
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Le,
                result: result_ty,
                prim_kind: prim_kind_byte(ctx.typed, result_ty),
            });
        }
        BinOp::Ne => {
            lower_expr(ctx, left);
            lower_expr(ctx, right);
            ctx.emit(IrInst::BinOp {
                op: IrBinOp::Ne,
                result: result_ty,
                prim_kind: prim_kind_byte(ctx.typed, result_ty),
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
                    prim_kind: prim_kind_byte(ctx.typed, result_ty),
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
                    prim_kind: prim_kind_byte(ctx.typed, result_ty),
                });
            }
        }
        #[allow(unreachable_patterns)]
        _ => {
            // Reserved for future `BinOp` variants (`#[non_exhaustive]`).
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
    _result_ty: TypeId,
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
    let bool_ty_id = bool_ty(ctx.typed);
    let short_val = if op == BinOp::And {
        ctx.intern_const(IrConst::Bool(false))
    } else {
        ctx.intern_const(IrConst::Bool(true))
    };
    ctx.emit(IrInst::Const {
        index: short_val,
        ty: bool_ty_id,
        prim_kind: prim_kind_byte(ctx.typed, bool_ty_id),
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

/// Type of the slice/base for `base[index] = …` (when `target` is a single index postfix).
fn slice_ty_for_index_assign_target(ctx: &LowerCtx<'_>, target: &Expr) -> Option<TypeId> {
    let Expr::Postfix { base, ops, .. } = target else {
        return None;
    };
    if ops.len() != 1 || !matches!(ops[0], PostfixOp::Index(_)) {
        return None;
    }
    type_id_for_index_base(ctx, base)
}

fn type_id_for_index_base(ctx: &LowerCtx<'_>, base: &ExprNode) -> Option<TypeId> {
    match &base.inner {
        Expr::Ident(ident) => ctx.layout.binding(ident.symbol).map(|b| b.ty),
        _ => None,
    }
}

fn slice_element_ty(ctx: &LowerCtx<'_>, slice_ty: TypeId) -> Option<TypeId> {
    match ctx.typed.types.get(slice_ty) {
        Ty::Slice(elem) => Some(*elem),
        _ => None,
    }
}

fn prim_kind_for_index_store(
    ctx: &LowerCtx<'_>,
    value_ty: TypeId,
    target: &Expr,
) -> Option<PrimitiveKind> {
    primitive_kind_for_type(&ctx.typed.types, value_ty).or_else(|| {
        slice_ty_for_index_assign_target(ctx, target)
            .and_then(|slice_ty| slice_element_ty(ctx, slice_ty))
            .and_then(|elem| primitive_kind_for_type(&ctx.typed.types, elem))
    })
}

/// Lowers assignment (`=`, `+=`, …) into store or compound-op IR.
pub(crate) fn lower_assign_expr(
    ctx: &mut LowerCtx<'_>,
    target: &ExprNode,
    value: &ExprNode,
    _result_ty: TypeId,
) {
    lower_assign_target(ctx, &target.inner);
    let value_ty = if let Some(elem_ty) = slice_ty_for_index_assign_target(ctx, &target.inner)
        .and_then(|slice_ty| slice_element_ty(ctx, slice_ty))
    {
        lower_expr_with_type(ctx, value, elem_ty);
        elem_ty
    } else {
        lower_expr_typed(ctx, value)
    };
    match &target.inner {
        Expr::Ident(ident) => {
            if let Some(binding) = ctx.layout.binding(ident.symbol) {
                ctx.emit(IrInst::StoreLocal {
                    slot: binding.slot,
                    ty: binding.ty,
                    prim_kind: prim_kind_byte(ctx.typed, binding.ty),
                });
            }
        }
        Expr::Unary {
            op: UnaryOp::Deref, ..
        } => {
            if let Some(kind) = primitive_kind_for_type(&ctx.typed.types, value_ty) {
                ctx.emit(IrInst::PtrStore {
                    prim_kind: kind.as_u8(),
                    signed: primitive_load_signed(kind),
                });
            }
        }
        Expr::Postfix { base, ops } if ops.len() == 1 => {
            if let PostfixOp::Field(field) = &ops[0] {
                if let Some(def) = struct_def_from_base(ctx, base) {
                    let args = struct_args_from_base(ctx, base);
                    let type_id = ctx.typed.layout.type_id_for_named(def, &args).unwrap_or(0);
                    let field_index = ctx
                        .typed
                        .layout
                        .struct_field_index(def, field.symbol, &args)
                        .unwrap_or(0);
                    ctx.emit(IrInst::SetField {
                        type_id,
                        field_index,
                    });
                    if let Expr::Ident(ident) = &base.inner {
                        if binding_is_ref_to_struct(ctx, ident.symbol) {
                            ctx.emit(IrInst::Pop);
                        } else {
                            store_base_local(ctx, base);
                        }
                    } else {
                        store_base_local(ctx, base);
                    }
                }
            } else if matches!(ops[0], PostfixOp::Index(_)) {
                if let Some(kind) = prim_kind_for_index_store(ctx, value_ty, &target.inner) {
                    ctx.emit(IrInst::IndexStore {
                        prim_kind: kind.as_u8(),
                        signed: primitive_load_signed(kind),
                    });
                }
            }
        }
        _ => {}
    }
}

fn lower_assign_target(ctx: &mut LowerCtx<'_>, target: &Expr) {
    match target {
        Expr::Ident(_) => {}
        Expr::Unary {
            op: UnaryOp::Deref,
            operand,
            ..
        } => {
            lower_expr(ctx, operand);
        }
        Expr::Postfix { base, ops } if ops.len() == 1 => match &ops[0] {
            PostfixOp::Field(_) => emit_load_struct_base(ctx, base),
            PostfixOp::Index(idx) => {
                lower_expr(ctx, base);
                lower_expr(ctx, idx);
            }
            _ => {}
        },
        Expr::Literal(_) | Expr::Path(_) => {}
        _ => {}
    }
}

fn lower_postfix(
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

fn resolve_method_callee(ctx: &LowerCtx<'_>, base: &ExprNode, name: &Ident) -> Option<DefId> {
    let receiver_ty = match &base.inner {
        Expr::Ident(ident) => ctx.layout.binding(ident.symbol).map(|b| b.ty)?,
        _ => return None,
    };
    resolve_method_callee_for_ty(ctx, receiver_ty, name.symbol)
}

fn resolve_method_callee_for_ty(
    ctx: &LowerCtx<'_>,
    receiver_ty: TypeId,
    method: Symbol,
) -> Option<DefId> {
    let receiver_ty = concrete_method_receiver_ty(ctx, receiver_ty);
    let (type_def, implementer_args) = named_type_parts(ctx.typed, receiver_ty)?;
    let template = ctx
        .typed
        .layout
        .inherent_methods
        .get(&(type_def, method))
        .copied()
        .or_else(|| find_trait_method(ctx, type_def, &implementer_args, method))?;
    let mono_args = mono_args_for_current_fn(ctx).unwrap_or(implementer_args);
    Some(
        crate::typeck::specialized_fn_for_inst(ctx.typed, template, &mono_args).unwrap_or(template),
    )
}

fn concrete_method_receiver_ty(ctx: &LowerCtx<'_>, receiver_ty: TypeId) -> TypeId {
    let Ty::Named { def, .. } = ctx.typed.types.get(receiver_ty).clone() else {
        return receiver_ty;
    };
    if !ctx
        .typed
        .resolved
        .defs
        .get(def.index() as usize)
        .is_some_and(|d| d.kind == DefKind::GenericParam)
    {
        return receiver_ty;
    }
    concrete_type_for_generic_param(ctx, def).unwrap_or(receiver_ty)
}

fn concrete_type_for_generic_param(ctx: &LowerCtx<'_>, param_def: DefId) -> Option<TypeId> {
    let base_fn = ctx.typed.specialized_from.get(&ctx.layout.def)?;
    let inst = ctx
        .typed
        .mono_insts
        .iter()
        .find(|i| i.base_fn == *base_fn)?;
    let param_defs = crate::typeck::generic_param_defs_for_fn_base(&ctx.typed.resolved, *base_fn)?;
    param_defs
        .iter()
        .zip(inst.args.iter())
        .find_map(|(p, ty)| (*p == param_def).then_some(*ty))
}

fn mono_args_for_current_fn(ctx: &LowerCtx<'_>) -> Option<Vec<TypeId>> {
    let base_fn = ctx.typed.specialized_from.get(&ctx.layout.def)?;
    ctx.typed
        .mono_insts
        .iter()
        .find(|i| i.base_fn == *base_fn)
        .map(|i| i.args.clone())
}

fn emit_ref_method_receiver(ctx: &mut LowerCtx<'_>, base: &ExprNode) {
    if let Expr::Ident(ident) = &base.inner
        && let Some(slot) = slot_for_symbol(ctx.layout, ident.symbol)
    {
        ctx.emit(IrInst::AddressOfLocal { slot });
    }
}

/// Stores the evaluated receiver value and passes `&mut` / `&` for method calls after field chains.
fn emit_ref_receiver_from_stack_value(ctx: &mut LowerCtx<'_>, callee: DefId, value_ty: TypeId) {
    if method_ref_receiver_ty(ctx, callee).is_none() {
        return;
    }
    let slot = ctx.next_match_temp();
    ctx.emit(IrInst::StoreLocal {
        slot,
        ty: value_ty,
        prim_kind: prim_kind_byte(ctx.typed, value_ty),
    });
    ctx.emit(IrInst::AddressOfLocal { slot });
}

fn single_method_with_ref_receiver(
    ctx: &LowerCtx<'_>,
    base: &ExprNode,
    ops: &[PostfixOp],
) -> Option<(DefId, TypeId)> {
    if ops.len() != 1 {
        return None;
    }
    let PostfixOp::Method { name, .. } = &ops[0] else {
        return None;
    };
    let callee = resolve_method_callee(ctx, base, name)?;
    let ref_ty = method_ref_receiver_ty(ctx, callee)?;
    Some((callee, ref_ty))
}

#[allow(clippy::too_many_lines)]
fn lower_postfix_inner(
    ctx: &mut LowerCtx<'_>,
    base: &ExprNode,
    ops: &[PostfixOp],
    result_ty: TypeId,
    postfix_expr_id: ExprId,
) {
    let static_call = ops.len() == 1
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
            emit_call_or_intrinsic(ctx, callee, result_ty, postfix_expr_id);
        }
        return;
    }

    let ref_method = single_method_with_ref_receiver(ctx, base, ops);
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
                    ctx.emit(IrInst::LoadAggViaLocalPtr);
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
                    ctx.emit(IrInst::GetField {
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
                    .or_else(|| ctx.typed.method_call_sites.get(&postfix_expr_id).copied())
                    .or_else(|| resolve_method_callee_for_ty(ctx, receiver_ty, name.symbol))
                    .or_else(|| resolve_method_callee(ctx, base, name));
                if let Some(callee) = callee {
                    if ref_method.is_none() {
                        emit_ref_receiver_from_stack_value(ctx, callee, receiver_ty);
                    }
                    for arg in args {
                        lower_expr(ctx, arg);
                    }
                    emit_call_or_intrinsic(ctx, callee, result_ty, postfix_expr_id);
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
                    ctx.emit(IrInst::MakeStruct {
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
                        ctx.emit(IrInst::MakeEnum {
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
                    ctx.emit(IrInst::CallIndirect {
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
                ctx.emit(IrInst::Index { result: result_ty });
            }
            PostfixOp::Try => {
                if let Some(meta) = ctx.typed.try_sites.get(&postfix_expr_id) {
                    lower_try(ctx, meta);
                    receiver_ty = result_ty;
                }
            }
            _ => {}
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
    ctx.emit(IrInst::StoreLocal {
        slot: temp,
        ty: temp_ty,
        prim_kind: prim,
    });

    let succ_block = ctx.fresh_block();
    let fail_block = ctx.fresh_block();
    let cont_block = ctx.fresh_block();

    ctx.emit(IrInst::LoadLocal {
        slot: temp,
        ty: temp_ty,
        prim_kind: prim,
    });
    ctx.emit(IrInst::MatchTag {
        type_id: bytecode_type_id,
        variant_tag: meta.success_tag,
    });
    ctx.emit(IrInst::JumpIf {
        then_block: succ_block,
        else_block: fail_block,
    });

    ctx.set_current(succ_block);
    ctx.emit(IrInst::LoadLocal {
        slot: temp,
        ty: temp_ty,
        prim_kind: prim,
    });
    ctx.emit(IrInst::GetField {
        type_id: bytecode_type_id,
        field_index: 0,
        result: meta.success_ty,
    });
    ctx.emit(IrInst::Jump { target: cont_block });

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
    ctx.emit(IrInst::LoadLocal {
        slot: temp,
        ty: temp_ty,
        prim_kind: prim,
    });
    ctx.emit(IrInst::Return {
        ty: ctx.layout.return_type,
    });
}

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
    ctx.emit(IrInst::LoadLocal {
        slot: temp,
        ty: temp_ty,
        prim_kind: prim,
    });
    ctx.emit(IrInst::GetField {
        type_id: bytecode_type_id,
        field_index: 0,
        result: *err_in_ty,
    });
    ctx.emit(IrInst::Call {
        callee,
        ret: *err_out_ty,
    });
    let Ty::Named {
        def: ret_enum_def,
        args: ret_enum_args,
    } = ctx.typed.types.get(*return_result_ty).clone()
    else {
        debug_assert!(false, "missing return Result type for `?` conversion");
        return;
    };
    let Some(ret_bytecode_type_id) = ctx
        .typed
        .layout
        .type_id_for_named(ret_enum_def, &ret_enum_args)
    else {
        debug_assert!(false, "missing bytecode type id for return Result");
        return;
    };
    let Some(err_tag) = ctx.typed.std_kernel.failure_tag_for(
        &ctx.typed.layout,
        &ctx.typed.types,
        *return_result_ty,
    ) else {
        debug_assert!(false, "missing Err tag for return Result");
        return;
    };
    ctx.emit(IrInst::MakeEnum {
        type_id: ret_bytecode_type_id,
        variant_tag: err_tag,
        payload_count: 1,
    });
    ctx.emit(IrInst::Return {
        ty: ctx.layout.return_type,
    });
}

fn size_of_bytes_for_specialized_fn(ctx: &LowerCtx<'_>) -> Option<u32> {
    let base = ctx.typed.specialized_from.get(&ctx.layout.def)?;
    let inst = ctx.typed.mono_insts.iter().find(|i| i.base_fn == *base)?;
    let elem = *inst.args.first()?;
    crate::typeck::type_byte_size(
        &ctx.typed.types,
        &ctx.typed.layout,
        &ctx.typed.resolved,
        elem,
    )
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
    if let Some(site) = ctx.typed.intrinsic_kernel.site_for_call(callee) {
        lower_intrinsic_call(ctx, site, result_ty, expr_id);
        return;
    }
    ctx.emit(IrInst::Call {
        callee,
        ret: result_ty,
    });
    if callee_returns_unit(ctx, callee, result_ty) {
        ctx.emit(IrInst::Pop);
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

fn lower_intrinsic_call(
    ctx: &mut LowerCtx<'_>,
    site: IntrinsicSite,
    result_ty: TypeId,
    expr_id: ExprId,
) {
    match site {
        IntrinsicSite::AllocBytes => {
            ctx.emit(IrInst::Alloc { result: result_ty });
        }
        IntrinsicSite::DeallocBytes => {
            ctx.emit(IrInst::Free);
        }
        IntrinsicSite::SliceFromRawParts => {
            let elem_kind = match ctx.typed.types.get(result_ty) {
                Ty::Slice(elem) => primitive_kind_for_type(&ctx.typed.types, *elem).map_or(
                    phx_bytecode::SLOT_KIND_AGG,
                    phx_bytecode::PrimitiveKind::as_u8,
                ),
                _ => phx_bytecode::SLOT_KIND_AGG,
            };
            ctx.emit(IrInst::MakeSliceFromPtr { elem_kind });
        }
        IntrinsicSite::SizeOf => {
            let bytes = ctx
                .typed
                .size_of_literals
                .get(&expr_id)
                .copied()
                .or_else(|| size_of_bytes_for_specialized_fn(ctx))
                .unwrap_or(0);
            let idx = ctx.intern_const(IrConst::Int(
                i128::from(bytes),
                phx_bytecode::PrimitiveKind::U32,
            ));
            ctx.emit(IrInst::Const {
                index: idx,
                ty: result_ty,
                prim_kind: phx_bytecode::PrimitiveKind::U32.as_u8(),
            });
        }
    }
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
            ctx.emit(IrInst::BinOp {
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

fn find_trait_method(
    ctx: &LowerCtx<'_>,
    type_def: DefId,
    implementer_args: &[TypeId],
    method: Symbol,
) -> Option<DefId> {
    let mut matches: Vec<DefId> = ctx
        .typed
        .layout
        .trait_methods
        .iter()
        .filter(|((key, m), _)| {
            key.implementer == type_def
                && key.implementer_args.as_slice() == implementer_args
                && *m == method
        })
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

fn named_type_parts(typed: &TypedProgram, ty: TypeId) -> Option<(DefId, Vec<TypeId>)> {
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

fn tuple_struct_repr_cast(ctx: &LowerCtx<'_>, from: TypeId, to: TypeId) -> bool {
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

fn path_or_ident_node_id(expr: &Expr) -> Option<phx_syntax::AstNodeId> {
    match expr {
        Expr::Ident(ident) => Some(ident.id),
        Expr::Path(path) if path.segments.len() == 1 => match &path.segments[0] {
            PathSegment::Ident(ident) => Some(ident.id),
            PathSegment::Type(seg) => Some(seg.name.id),
            _ => None,
        },
        _ => None,
    }
}

fn struct_def_from_ty(ctx: &LowerCtx<'_>, ty: TypeId) -> Option<DefId> {
    match ctx.typed.types.get(ty) {
        Ty::Named { def, .. } if ctx.typed.layout.structs.contains_key(def) => Some(*def),
        Ty::Ref { inner, .. } => struct_def_from_ty(ctx, *inner),
        _ => None,
    }
}

fn struct_args_from_base(ctx: &LowerCtx<'_>, base: &ExprNode) -> Vec<TypeId> {
    if let Expr::Ident(ident) = &base.inner {
        if let Some(binding) = ctx.layout.binding(ident.symbol) {
            if let Ty::Named { args, .. } = ctx.typed.types.get(binding.ty) {
                return args.clone();
            }
        }
    }
    Vec::new()
}

fn struct_def_from_base(ctx: &LowerCtx<'_>, base: &ExprNode) -> Option<DefId> {
    if let Expr::Ident(ident) = &base.inner {
        if let Some(binding) = ctx.layout.binding(ident.symbol) {
            return struct_def_from_ty(ctx, binding.ty);
        }
    }
    None
}

fn binding_is_ref_to_struct(ctx: &LowerCtx<'_>, symbol: Symbol) -> bool {
    ctx.layout
        .binding(symbol)
        .is_some_and(|b| matches!(ctx.typed.types.get(b.ty), Ty::Ref { inner, .. } if struct_def_from_ty(ctx, *inner).is_some()))
}

fn emit_load_struct_base(ctx: &mut LowerCtx<'_>, base: &ExprNode) {
    let ty = ctx.expr_ty();
    let expr_id = ExprId::from_raw(ctx.next_expr - 1);
    if let Expr::Ident(ident) = &base.inner {
        if binding_is_ref_to_struct(ctx, ident.symbol) {
            lower_ident(ctx, *ident, ty);
            ctx.emit(IrInst::LoadAggViaLocalPtr);
            return;
        }
    }
    lower_expr_inner(ctx, &base.inner, ty, expr_id);
}

fn store_base_local(ctx: &mut LowerCtx<'_>, base: &ExprNode) {
    if let Expr::Ident(ident) = &base.inner
        && let Some(binding) = ctx.layout.binding(ident.symbol)
        && !matches!(ctx.typed.types.get(binding.ty), Ty::Ref { .. })
    {
        ctx.emit(IrInst::StoreLocal {
            slot: binding.slot,
            ty: binding.ty,
            prim_kind: prim_kind_byte(ctx.typed, binding.ty),
        });
    }
}

fn lower_if(
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
fn lower_if_condition_test(
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

fn lower_match(ctx: &mut LowerCtx<'_>, scrutinee: &ExprNode, arms: &[MatchArm]) {
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
            if let Some(index) = intern_literal(ctx, lit, temp_ty) {
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
            } else {
                ctx.emit(IrInst::Jump { target: fail_id });
            }
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
        #[allow(unreachable_patterns)]
        _ => {
            // Reserved for future `Pattern` variants (`#[non_exhaustive]`).
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
        _ => {}
    }
}

fn aggregate_deref_target(types: &crate::typeck::TypeInterner, ty: TypeId) -> bool {
    matches!(
        types.get(ty),
        Ty::Named { .. } | Ty::Tuple(_) | Ty::Array { .. }
    )
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

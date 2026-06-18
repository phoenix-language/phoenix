//! Lower function definitions to [`IrFunction`](crate::ir::IrFunction).

use crate::ir::{IrConst, IrFunction, IrFunctionId};
use crate::lower::ctx::LowerCtx;
use crate::lower::stmt::{lower_block_value, lower_function_return};
use crate::resolver::{DefId, DefKind};
use crate::typeck::{
    BindingKind, FunctionLayout, TypedProgram, is_generic_fn_template,
    is_generic_impl_method_template, lookup_function,
};
use phx_diagnostics::{LowerBag, LowerError};

/// Lowers all functions in `typed`.
///
/// # Errors
///
/// Returns [`LowerBag`] when any function body hits an internal lowering invariant violation.
pub fn lower_functions(
    typed: &TypedProgram,
    constants: &mut Vec<IrConst>,
    bag: &mut LowerBag,
) -> Result<Vec<IrFunction>, LowerBag> {
    let mut functions = Vec::new();
    for layout in &typed.functions {
        if is_generic_template(typed, layout.def) {
            continue;
        }
        if typed.lang_items.is_intrinsic_fn(layout.def) {
            continue;
        }
        if let Some(f) = lower_one_function(typed, layout, functions.len(), constants, bag) {
            functions.push(f);
        }
    }
    let functions = functions;
    if bag.has_errors() {
        Err(std::mem::take(bag))
    } else {
        Ok(functions)
    }
}

/// Lowers a single function layout to IR.
pub(crate) fn lower_one_function(
    typed: &TypedProgram,
    layout: &FunctionLayout,
    index: usize,
    constants: &mut Vec<IrConst>,
    bag: &mut LowerBag,
) -> Option<IrFunction> {
    let source = lookup_function(typed, layout.def)?;
    let def_record = typed.resolved.defs.get(layout.def.index() as usize)?;
    // Specialized clones keep the template AST and resolution keys on the template module.
    let module = typed
        .specialized_from
        .get(&layout.def)
        .and_then(|base| typed.resolved.defs.get(base.index() as usize))
        .map_or(def_record.module, |base| base.module);
    let mut ctx = LowerCtx::new(typed, module, layout, constants, bag, source.body.span);
    ctx.set_site(source.body.span);
    let skip_template_body = layout.expr_start >= layout.expr_end
        && (is_generic_impl_method_template(typed, layout.def)
            || is_deferred_impl_method_template(typed, layout));
    if !skip_template_body {
        lower_block_value(&mut ctx, &source.body.inner);
    }
    lower_function_return(&mut ctx, &source.body, layout.return_type);
    ctx.finish_expr_cursor();
    if ctx.bag.has_errors() {
        return None;
    }

    let params: Vec<_> = layout
        .bindings
        .iter()
        .filter(|b| b.kind == BindingKind::Param)
        .map(|b| b.ty)
        .collect();

    let ir_index = if let Ok(idx) = u32::try_from(index) {
        idx
    } else {
        bag.push(
            module,
            LowerError::LimitExceeded {
                item: "functions",
                len: index,
            },
        );
        return None;
    };
    let pending_exits = ctx.pending_loop_exits.clone();
    let mut blocks = ctx.into_blocks();
    LowerCtx::patch_loop_exit_targets(&mut blocks, &pending_exits);
    Some(IrFunction {
        id: IrFunctionId::from_raw(ir_index),
        def: layout.def,
        params,
        return_type: layout.return_type,
        local_count: layout.local_count(),
        blocks,
    })
}

/// Returns true when `layout` is a generic impl method template whose body was not type-checked.
fn is_deferred_impl_method_template(typed: &TypedProgram, layout: &FunctionLayout) -> bool {
    if layout.expr_start < layout.expr_end {
        return false;
    }
    if typed.specialized_from.contains_key(&layout.def) {
        return false;
    }
    let Some(def) = typed.resolved.defs.get(layout.def.index() as usize) else {
        return false;
    };
    def.kind == DefKind::ImplMethod
}

/// Returns true when `def` is a generic template replaced by monomorphization.
fn is_generic_template(typed: &TypedProgram, def: DefId) -> bool {
    is_generic_fn_template(typed, def) || is_generic_impl_method_template(typed, def)
}

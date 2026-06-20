//! Function-body lowering: one [`IrFunction`](crate::ir::IrFunction) per typed layout.
//!
//! Iterates [`TypedProgram::functions`](crate::typeck::TypedProgram::functions), skips generic
//! templates and VM intrinsics, and drives each remaining body through [`LowerCtx`](crate::lower::ctx::LowerCtx).
//! Block statements use [`super::stmt::lower_block_value`]; the tail return is finalized by
//! [`super::stmt::lower_function_return`].
//!
//! ## Invariants
//!
//! - **Generic templates:** empty expression-range templates are omitted here; monomorphized copies
//!   carry real bodies (see [`is_generic_template`] and [`is_deferred_impl_method_template`]).
//! - **Specialized clones:** resolution keys for specialized functions may point at the template
//!   module; [`lower_one_function`] uses [`TypedProgram::specialized_from`] to pick the correct
//!   module for diagnostics.
//! - **Completion:** each successful lowering runs [`LowerCtx::finish_expr_cursor`] and patches
//!   loop exit placeholders before returning the [`IrFunction`].

use crate::ir::{IrConst, IrFunction, IrFunctionId};
use crate::lower::ctx::LowerCtx;
use crate::lower::stmt::{lower_block_value, lower_function_return};
use crate::resolver::{DefId, DefKind};
use crate::typeck::{
    BindingKind, FunctionLayout, TypedProgram, is_generic_fn_template,
    is_generic_impl_method_template, lookup_function,
};
use phx_diagnostics::{LowerBag, LowerError};

/// Lowers every non-template, non-intrinsic function in `typed` to IR.
///
/// Appends each [`IrFunction`] in layout order with dense [`IrFunctionId`] indices. Accumulates
/// lowering errors in `bag`; on any error the partial `functions` vector is discarded and the
/// taken bag is returned.
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

/// Lowers a single [`FunctionLayout`](crate::typeck::FunctionLayout) to an [`IrFunction`].
///
/// Builds a fresh [`LowerCtx`], optionally skips the AST body for deferred generic impl templates,
/// lowers statements, appends a fall-through return, verifies the expression cursor, and patches
/// loop exit targets. Returns `None` when lowering recorded errors in `bag` or when the function
/// index exceeds representable limits.
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

/// True when `layout` is a generic impl method template whose body was not type-checked.
///
/// Such layouts have an empty expression range (`expr_start >= expr_end`) and are not yet
/// specialized; lowering skips the AST body until a monomorphized copy exists.
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

/// True when `def` is a generic function or impl-method template replaced by monomorphization.
fn is_generic_template(typed: &TypedProgram, def: DefId) -> bool {
    is_generic_fn_template(typed, def) || is_generic_impl_method_template(typed, def)
}

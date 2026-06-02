//! Lower function definitions to [`IrFunction`](crate::ir::IrFunction).

use phx_syntax::ast::Node;
use phx_syntax::ast::decl::{Function, TopLevelDecl, TopLevelItem};

use crate::ir::{IrConst, IrFunction, IrFunctionId};
use crate::lower::ctx::LowerCtx;
use crate::lower::stmt::{lower_block_value, lower_function_return};
use crate::resolver::{DefId, DefKind};
use crate::typeck::{BindingKind, FunctionLayout, TypedProgram};

/// Lowers all functions in `typed`.
#[must_use]
pub fn lower_functions(typed: &TypedProgram, constants: &mut Vec<IrConst>) -> Vec<IrFunction> {
    typed
        .functions
        .iter()
        .enumerate()
        .filter_map(|(index, layout)| lower_one_function(typed, layout, index, constants))
        .collect()
}

fn lower_one_function(
    typed: &TypedProgram,
    layout: &FunctionLayout,
    index: usize,
    constants: &mut Vec<IrConst>,
) -> Option<IrFunction> {
    let source = find_function(&typed.resolved.program.items, layout.def, typed)?;
    let mut ctx = LowerCtx::new(typed, layout, constants);
    lower_block_value(&mut ctx, &source.body.inner);
    lower_function_return(&mut ctx, &source.body.inner, layout.return_type);

    let params: Vec<_> = layout
        .bindings
        .iter()
        .filter(|b| b.kind == BindingKind::Param)
        .map(|b| b.ty)
        .collect();

    let ir_index = u32::try_from(index).unwrap_or(u32::MAX);
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

fn find_function<'a>(
    items: &'a [Node<TopLevelItem>],
    def: DefId,
    typed: &TypedProgram,
) -> Option<&'a Function> {
    for item in items {
        match &item.inner.decl {
            TopLevelDecl::Function(f) if fn_def_id(typed, f) == Some(def) => return Some(f),
            TopLevelDecl::Impl { members, .. } => {
                for m in members {
                    if fn_def_id(typed, m) == Some(def) {
                        return Some(m);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn fn_def_id(typed: &TypedProgram, f: &Function) -> Option<DefId> {
    typed
        .resolved
        .defs
        .iter()
        .enumerate()
        .find(|(_, d)| d.name == f.name.symbol && d.kind == DefKind::Fn)
        .and_then(|(i, _)| u32::try_from(i).ok().map(DefId::from_raw))
}

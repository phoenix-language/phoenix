//! Lower function definitions to [`IrFunction`](crate::ir::IrFunction).

use phx_syntax::ast::decl::{Function, TopLevelDecl, TopLevelItem};
use phx_syntax::ast::Node;

use crate::ir::{IrFunction, IrFunctionId};
use crate::lower::ctx::LowerCtx;
use crate::lower::stmt::{lower_block_value, lower_function_return};
use crate::resolver::{DefId, DefKind};
use crate::typeck::{BindingKind, FunctionLayout, TypedProgram};

/// Lowers all functions in `typed`.
#[must_use]
pub fn lower_functions(typed: &TypedProgram) -> Vec<IrFunction> {
    typed
        .functions
        .iter()
        .enumerate()
        .filter_map(|(index, layout)| lower_one_function(typed, layout, index))
        .collect()
}

fn lower_one_function(
    typed: &TypedProgram,
    layout: &FunctionLayout,
    index: usize,
) -> Option<IrFunction> {
    let source = find_function(&typed.resolved.program.items, layout.def, typed)?;
    let mut ctx = LowerCtx::new(typed, layout);
    lower_block_value(&mut ctx, &source.body.inner);
    lower_function_return(&mut ctx, &source.body.inner, layout.return_type);

    let params: Vec<_> = layout
        .bindings
        .iter()
        .filter(|b| b.kind == BindingKind::Param)
        .map(|b| b.ty)
        .collect();

    let ir_index = u32::try_from(index).unwrap_or(u32::MAX);
    Some(IrFunction {
        id: IrFunctionId::from_raw(ir_index),
        def: layout.def,
        params,
        return_type: layout.return_type,
        local_count: layout.local_count(),
        blocks: ctx.into_blocks(),
    })
}

fn find_function<'a>(
    items: &'a [Node<TopLevelItem>],
    def: DefId,
    typed: &TypedProgram,
) -> Option<&'a Function> {
    for item in items {
        let TopLevelDecl::Function(f) = &item.inner.decl else {
            continue;
        };
        if fn_def_id(typed, f) == Some(def) {
            return Some(f);
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

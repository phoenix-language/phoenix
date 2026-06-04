//! Lower function definitions to [`IrFunction`](crate::ir::IrFunction).

use phx_syntax::ast::decl::{Function, TopLevelDecl};

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

/// Lowers a single function layout to IR.
pub(crate) fn lower_one_function(
    typed: &TypedProgram,
    layout: &FunctionLayout,
    index: usize,
    constants: &mut Vec<IrConst>,
) -> Option<IrFunction> {
    let source = find_function_in_crate(typed, layout.def)?;
    let module = typed
        .resolved
        .defs
        .get(layout.def.index() as usize)
        .map_or(0, |d| d.module);
    let mut ctx = LowerCtx::new(typed, module, layout, constants);
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

fn find_function_in_crate(typed: &TypedProgram, def: DefId) -> Option<&Function> {
    let def_record = typed.resolved.defs.get(def.index() as usize)?;
    let module = typed
        .resolved
        .modules
        .iter()
        .find(|m| m.id == def_record.module)?;
    for item in &module.program.items {
        match &item.inner.decl {
            TopLevelDecl::Function(f) if def_matches(typed, f, def) => return Some(f),
            TopLevelDecl::Impl { members, .. } => {
                for m in members {
                    if def_matches(typed, m, def) {
                        return Some(m);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn def_matches(typed: &TypedProgram, f: &Function, def: DefId) -> bool {
    let lookup = typed.specialized_from.get(&def).copied().unwrap_or(def);
    typed
        .resolved
        .defs
        .get(lookup.index() as usize)
        .is_some_and(|d| d.name == f.name.symbol && d.kind == DefKind::Fn)
}

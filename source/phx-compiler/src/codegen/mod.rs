//! IR to PHX0 bytecode codegen.
//!
//! Flattens per-function CFGs into the code section, builds constant and function tables,
//! and sets `entry_function_id` to `main`.

mod const_pool;
mod emit;

use phx_bytecode::{BytecodeModule, FileHeader, FunctionRecord, FunctionTable, TypeTable};
use std::collections::HashMap;

use crate::ir::IrModule;
use crate::resolver::DefId;

pub use const_pool::ConstPoolBuilder;

/// Lowers `ir` to a [`BytecodeModule`] ready for [`phx_bytecode::verify`] and the VM.
#[must_use]
pub fn codegen(ir: &IrModule) -> BytecodeModule {
    let def_to_fn: HashMap<DefId, u32> =
        ir.functions.iter().map(|f| (f.def, f.id.index())).collect();
    let fn_arity: HashMap<u32, u16> = ir
        .functions
        .iter()
        .map(|f| {
            (
                f.id.index(),
                u16::try_from(f.params.len()).unwrap_or(u16::MAX),
            )
        })
        .collect();

    let mut pool = ConstPoolBuilder::new();
    let mut code = Vec::new();
    let mut records = Vec::new();

    for func in &ir.functions {
        let offset = u32::try_from(code.len()).unwrap_or(u32::MAX);
        let emitted = emit::emit_function(func, &mut pool, &def_to_fn, &fn_arity);
        let len = u32::try_from(emitted.code.len()).unwrap_or(u32::MAX);
        records.push(FunctionRecord {
            function_id: func.id.index(),
            name_symbol_id: 0,
            arity: u16::try_from(func.params.len()).unwrap_or(u16::MAX),
            local_count: u16::try_from(func.local_count).unwrap_or(u16::MAX),
            stack_max: emitted.stack_max,
            flags: 0,
            code_offset: offset,
            code_len: len,
            return_type_id: 0,
        });
        code.extend_from_slice(&emitted.code);
    }

    let entry_function_id = ir
        .entry
        .and_then(|main| def_to_fn.get(&main).copied())
        .unwrap_or(0);

    let constants = pool.finish();
    BytecodeModule {
        header: FileHeader {
            entry_function_id,
            section_count: 4,
            ..FileHeader::new(4, entry_function_id)
        },
        constants,
        types: TypeTable::default(),
        functions: FunctionTable { functions: records },
        code,
    }
}

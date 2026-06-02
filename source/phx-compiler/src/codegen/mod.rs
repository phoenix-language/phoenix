//! IR to PHX0 bytecode codegen.
//!
//! Flattens per-function CFGs into the code section, builds constant and function tables,
//! and sets `entry_function_id` to `main`.

mod const_pool;
mod emit;

use phx_bytecode::{
    BytecodeModule, FileHeader, FunctionRecord, FunctionTable, TypeKind, TypeRecord, TypeTable,
};
use std::collections::HashMap;

use crate::ir::IrModule;
use crate::resolver::DefId;
use crate::typeck::ProgramLayout;

pub use const_pool::ConstPoolBuilder;

/// Builds the bytecode types section from typeck layout tables.
#[must_use]
pub fn build_type_table(layout: &ProgramLayout) -> TypeTable {
    let mut records = Vec::new();

    for (&def, sl) in &layout.structs {
        let Some(type_id) = layout.type_id(def) else {
            continue;
        };
        let mut aux = Vec::new();
        let field_count = u32::try_from(sl.fields.len()).unwrap_or(u32::MAX);
        aux.extend_from_slice(&field_count.to_le_bytes());
        for (name, _ty) in &sl.fields {
            aux.extend_from_slice(&name.index().to_le_bytes());
            aux.extend_from_slice(&0u32.to_le_bytes());
        }
        records.push(TypeRecord {
            type_id,
            kind: TypeKind::Struct,
            aux,
        });
    }

    for (&def, el) in &layout.enums {
        let Some(type_id) = layout.type_id(def) else {
            continue;
        };
        let mut aux = Vec::new();
        let variant_count = u32::try_from(el.variants.len()).unwrap_or(u32::MAX);
        aux.extend_from_slice(&variant_count.to_le_bytes());
        for v in &el.variants {
            aux.extend_from_slice(&v.name.index().to_le_bytes());
            aux.extend_from_slice(&v.tag.to_le_bytes());
            let payload_len = u32::try_from(v.kind.payload_len()).unwrap_or(u32::MAX);
            aux.extend_from_slice(&payload_len.to_le_bytes());
        }
        records.push(TypeRecord {
            type_id,
            kind: TypeKind::Enum,
            aux,
        });
    }

    TypeTable { records }
}

/// Lowers `ir` to a [`BytecodeModule`] ready for [`phx_bytecode::verify`] and the VM.
#[must_use]
pub fn codegen(ir: &IrModule, layout: &ProgramLayout) -> BytecodeModule {
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
        types: build_type_table(layout),
        functions: FunctionTable { functions: records },
        code,
    }
}

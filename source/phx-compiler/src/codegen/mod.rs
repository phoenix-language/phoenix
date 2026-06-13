//! IR to PHX0 bytecode codegen.
//!
//! Flattens per-function CFGs into the code section, builds constant and function tables,
//! and sets `entry_function_id` to `main`.

mod const_pool;
mod emit;
mod error;

use phx_bytecode::{
    BytecodeModule, ENTRY_NONE, FileHeader, FunctionLocalLayout, FunctionRecord, FunctionTable,
    LocalLayoutTable, LocalSlotKind, TypeKind, TypeRecord, TypeTable,
};
use std::collections::HashMap;

use crate::PxiType;
use crate::ir::IrModule;
use crate::resolver::DefId;
use crate::typeck::{
    BindingKind, ProgramLayout, Ty, TypeInterner, TypedProgram, slot_kind_for_binding,
};

pub use const_pool::ConstPoolBuilder;
pub use error::CodegenError;

/// Builds the bytecode types section from typeck layout tables.
#[must_use]
pub fn build_type_table(layout: &ProgramLayout, types: &TypeInterner) -> TypeTable {
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

    for (&fn_ty, &type_id) in &layout.fn_sig_ids {
        let param_count = match types.get(fn_ty) {
            Ty::Fn { params, .. } => u32::try_from(params.len()).unwrap_or(u32::MAX),
            _ => 0,
        };
        records.push(TypeRecord {
            type_id,
            kind: TypeKind::FnSig,
            aux: param_count.to_le_bytes().to_vec(),
        });
    }

    TypeTable { records }
}

fn build_fn_arity_map(
    typed: &TypedProgram,
    ir: &IrModule,
    global_fn: &HashMap<DefId, u32>,
) -> HashMap<u32, u16> {
    let mut map = HashMap::new();
    for (&def, &fn_id) in global_fn {
        let param_count = if let Some(f) = ir.functions.iter().find(|f| f.def == def) {
            f.params.len()
        } else if let Some(fl) = typed.functions.iter().find(|f| f.def == def) {
            fl.bindings
                .iter()
                .filter(|b| b.kind == BindingKind::Param)
                .count()
        } else if let Some(PxiType::Fn { params, .. }) = typed.resolved.import_types.get(&def) {
            params.len()
        } else {
            continue;
        };
        if let Ok(arity) = u16::try_from(param_count) {
            map.insert(fn_id, arity);
        }
    }
    map
}

/// Lowers `ir` to a [`BytecodeModule`] ready for [`phx_bytecode::verify`] and the VM.
///
/// # Errors
///
/// Returns `CodegenError` when section sizes exceed `u32::MAX`.
pub fn codegen(ir: &IrModule, typed: &TypedProgram) -> Result<BytecodeModule, CodegenError> {
    use error::u32_section;
    let layout = &typed.layout;
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
    pool.fill_from_ir(&ir.constants);
    let mut code = Vec::new();
    let mut records = Vec::new();

    for func in &ir.functions {
        let offset = u32_section("code_offset", code.len())?;
        let emitted = emit::emit_function(func, &mut pool, &def_to_fn, &fn_arity);
        let len = u32_section("code_len", emitted.code.len())?;
        records.push(FunctionRecord {
            function_id: func.id.index(),
            name_symbol_id: 0,
            arity: u16::try_from(func.params.len()).map_err(|_| CodegenError::SectionTooLarge {
                section: "arity",
                len: func.params.len(),
            })?,
            local_count: u16::try_from(func.local_count).map_err(|_| {
                CodegenError::SectionTooLarge {
                    section: "local_count",
                    len: func.local_count as usize,
                }
            })?,
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
    let local_layouts = build_local_layouts(ir, typed);
    Ok(BytecodeModule {
        header: FileHeader {
            entry_function_id,
            section_count: 5,
            ..FileHeader::new(5, entry_function_id)
        },
        constants,
        types: build_type_table(layout, &typed.types),
        functions: FunctionTable { functions: records },
        code,
        local_layouts,
    })
}

/// Codegens one module's IR slice using global function ids for [`crate::ir::IrInst::Call`].
///
/// # Errors
///
/// Returns `CodegenError` when section sizes exceed `u32::MAX`.
#[allow(clippy::implicit_hasher)]
pub fn codegen_module(
    ir: &IrModule,
    typed: &TypedProgram,
    global_fn: &HashMap<DefId, u32>,
    is_entry_module: bool,
) -> Result<BytecodeModule, CodegenError> {
    use error::u32_section;
    let layout = &typed.layout;
    let def_to_fn = global_fn;
    let fn_arity = build_fn_arity_map(typed, ir, global_fn);

    let mut pool = ConstPoolBuilder::new();
    pool.fill_from_ir(&ir.constants);
    let mut code = Vec::new();
    let mut records = Vec::new();

    for func in &ir.functions {
        let fn_id = global_fn.get(&func.def).copied().unwrap_or(func.id.index());
        let offset = u32_section("code_offset", code.len())?;
        let emitted = emit::emit_function(func, &mut pool, def_to_fn, &fn_arity);
        let len = u32_section("code_len", emitted.code.len())?;
        records.push(FunctionRecord {
            function_id: fn_id,
            name_symbol_id: 0,
            arity: u16::try_from(func.params.len()).map_err(|_| CodegenError::SectionTooLarge {
                section: "arity",
                len: func.params.len(),
            })?,
            local_count: u16::try_from(func.local_count).map_err(|_| {
                CodegenError::SectionTooLarge {
                    section: "local_count",
                    len: func.local_count as usize,
                }
            })?,
            stack_max: emitted.stack_max,
            flags: 0,
            code_offset: offset,
            code_len: len,
            return_type_id: 0,
        });
        code.extend_from_slice(&emitted.code);
    }

    let entry_function_id = if is_entry_module {
        ir.entry
            .and_then(|main| global_fn.get(&main).copied())
            .unwrap_or(ENTRY_NONE)
    } else {
        ENTRY_NONE
    };

    let constants = pool.finish();
    let mut layouts = Vec::new();
    for func in &ir.functions {
        let Some(fl) = typed.functions.iter().find(|f| f.def == func.def) else {
            continue;
        };
        let fn_id = global_fn.get(&func.def).copied().unwrap_or(func.id.index());
        let slots: Vec<LocalSlotKind> = fl
            .bindings
            .iter()
            .map(|b| slot_kind_for_binding(&typed.types, b.ty))
            .collect();
        layouts.push(FunctionLocalLayout {
            function_id: fn_id,
            slots,
        });
    }

    Ok(BytecodeModule {
        header: FileHeader {
            entry_function_id,
            section_count: 5,
            ..FileHeader::new(5, entry_function_id)
        },
        constants,
        types: build_type_table(layout, &typed.types),
        functions: FunctionTable { functions: records },
        code,
        local_layouts: LocalLayoutTable { layouts },
    })
}

fn build_local_layouts(ir: &IrModule, typed: &TypedProgram) -> LocalLayoutTable {
    let mut layouts = Vec::new();
    for func in &ir.functions {
        let Some(fl) = typed.functions.iter().find(|f| f.def == func.def) else {
            continue;
        };
        let slots: Vec<LocalSlotKind> = fl
            .bindings
            .iter()
            .map(|b| slot_kind_for_binding(&typed.types, b.ty))
            .collect();
        layouts.push(FunctionLocalLayout {
            function_id: func.id.index(),
            slots,
        });
    }
    LocalLayoutTable { layouts }
}

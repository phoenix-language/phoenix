//! IR to PHX0 bytecode codegen.
//!
//! Flattens per-function CFGs into the code section, builds constant and function tables,
//! and sets `entry_function_id` to `main` or [`ENTRY_NONE`] when no entry is configured.

mod const_pool;
mod emit;
mod error;

use phx_bytecode::{
    BytecodeModule, ENTRY_NONE, FileHeader, FunctionLocalLayout, FunctionRecord, FunctionTable,
    LocalLayoutTable, LocalSlotKind, PHX0_HAS_DEBUG, PcSpanEntry, PcSpanTable, TypeKind,
    TypeRecord, TypeTable,
};
use std::collections::{HashMap, HashSet};

use crate::PxiType;
use crate::ir::{IrFunction, IrInst, IrModule};
use crate::resolver::DefId;
use crate::typeck::{
    BindingKind, ProgramLayout, Ty, TypeInterner, TypedProgram, slot_kind_for_binding,
};

pub use const_pool::ConstPoolBuilder;
pub use error::CodegenError;

fn emit_pc_spans_enabled() -> bool {
    cfg!(debug_assertions)
}

fn file_id_for_path(table: &mut PcSpanTable, path: Option<&str>) -> u32 {
    let Some(path) = path else {
        return 0;
    };
    if let Some(pos) = table.files.iter().position(|existing| existing == path) {
        return u32::try_from(pos).unwrap_or(0);
    }
    let id = table.files.len();
    table.files.push(path.to_owned());
    u32::try_from(id).unwrap_or(0)
}

fn record_emitted_pc_spans(
    table: &mut PcSpanTable,
    function_id: u32,
    file_id: u32,
    rows: &[(u32, phx_diagnostics::Span)],
) {
    for (pc, span) in rows {
        table.push_sorted(PcSpanEntry::new(
            function_id,
            *pc,
            file_id,
            span.start,
            span.end,
        ));
    }
}

fn module_header(entry_function_id: u32, pc_spans: &PcSpanTable) -> FileHeader {
    let section_count = u32::from(5u8.saturating_add(u8::from(!pc_spans.is_empty())));
    let flags = if pc_spans.is_empty() {
        0
    } else {
        PHX0_HAS_DEBUG
    };
    FileHeader {
        entry_function_id,
        section_count,
        flags,
        ..FileHeader::new(section_count, entry_function_id)
    }
}

/// Maps an IR function return type to the bytecode `return_type_id` field.
///
/// Uses [`IrInst::Return`] types when present so metadata matches what lowering
/// actually places on the stack; falls back to the function signature otherwise.
/// Unit returns use id `0` (zero stack cells at `RETURN`); all other returns use `1`.
fn bytecode_return_type_id(func: &IrFunction, typed: &TypedProgram) -> u32 {
    let return_tys: Vec<_> = func
        .blocks
        .iter()
        .flat_map(|block| &block.insts)
        .filter_map(|spanned| match &spanned.inst {
            IrInst::Return { ty } => Some(*ty),
            _ => None,
        })
        .collect();
    let unit_return = if return_tys.is_empty() {
        matches!(typed.types.get(func.return_type), Ty::Unit)
    } else {
        return_tys
            .iter()
            .all(|id| matches!(typed.types.get(*id), Ty::Unit))
    };
    u32::from(!unit_return)
}

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

/// Collects layout type ids referenced by aggregate and indirect-call IR in `ir`.
#[must_use]
pub fn collect_type_ids_from_ir(ir: &IrModule) -> HashSet<u32> {
    let mut used = HashSet::new();
    for func in &ir.functions {
        collect_type_ids_from_function(func, &mut used);
    }
    used
}

fn collect_type_ids_from_function(func: &IrFunction, used: &mut HashSet<u32>) {
    for block in &func.blocks {
        for spanned in &block.insts {
            match &spanned.inst {
                IrInst::MakeStruct { type_id, .. }
                | IrInst::MakeEnum { type_id, .. }
                | IrInst::GetField { type_id, .. }
                | IrInst::SetField { type_id, .. }
                | IrInst::MatchTag { type_id, .. } => {
                    used.insert(*type_id);
                }
                IrInst::CallIndirect { sig_type_id, .. } => {
                    used.insert(*sig_type_id);
                }
                IrInst::Const { .. }
                | IrInst::LoadLocal { .. }
                | IrInst::StoreLocal { .. }
                | IrInst::BinOp { .. }
                | IrInst::Call { .. }
                | IrInst::MakeFnPtr { .. }
                | IrInst::Return { .. }
                | IrInst::Jump { .. }
                | IrInst::JumpIf { .. }
                | IrInst::Cast { .. }
                | IrInst::Neg { .. }
                | IrInst::Not { .. }
                | IrInst::BitNot { .. }
                | IrInst::MakeTuple { .. }
                | IrInst::MakeArray { .. }
                | IrInst::Index { .. }
                | IrInst::IndexStore { .. }
                | IrInst::PtrLoad { .. }
                | IrInst::AddressOfLocal { .. }
                | IrInst::LoadAggViaLocalPtr
                | IrInst::Alloc { .. }
                | IrInst::PtrStore { .. }
                | IrInst::Free
                | IrInst::Pop
                | IrInst::MakeStr { .. }
                | IrInst::StrAsSlice
                | IrInst::SliceLen
                | IrInst::TrapGivenMismatch
                | IrInst::DropLocal { .. }
                | IrInst::MakeSlice { .. }
                | IrInst::MakeSliceFromPtr { .. } => {}
            }
        }
    }
}

/// Builds a module-local type table containing only ids in `used`, with dense local ids.
#[must_use]
pub fn build_module_type_table(
    layout: &ProgramLayout,
    types: &TypeInterner,
    used: &HashSet<u32>,
) -> (TypeTable, HashMap<u32, u32>) {
    let mut records = Vec::new();
    let mut remap = HashMap::new();

    let mut push = |global_id: u32, kind: TypeKind, aux: Vec<u8>| {
        if !used.contains(&global_id) {
            return;
        }
        if remap.contains_key(&global_id) {
            return;
        }
        let local_id = u32::try_from(records.len()).unwrap_or(u32::MAX);
        remap.insert(global_id, local_id);
        records.push(TypeRecord {
            type_id: local_id,
            kind,
            aux,
        });
    };

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
        push(type_id, TypeKind::Struct, aux);
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
        push(type_id, TypeKind::Enum, aux);
    }

    for (&fn_ty, &type_id) in &layout.fn_sig_ids {
        let param_count = match types.get(fn_ty) {
            Ty::Fn { params, .. } => u32::try_from(params.len()).unwrap_or(u32::MAX),
            _ => 0,
        };
        push(type_id, TypeKind::FnSig, param_count.to_le_bytes().to_vec());
    }

    for (key, sl) in &layout.specialized_structs {
        let Some(type_id) = layout.specialized_type_ids.get(key).copied() else {
            continue;
        };
        let mut aux = Vec::new();
        let field_count = u32::try_from(sl.fields.len()).unwrap_or(u32::MAX);
        aux.extend_from_slice(&field_count.to_le_bytes());
        for (name, _ty) in &sl.fields {
            aux.extend_from_slice(&name.index().to_le_bytes());
            aux.extend_from_slice(&0u32.to_le_bytes());
        }
        push(type_id, TypeKind::Struct, aux);
    }

    for (key, el) in &layout.specialized_enums {
        let Some(type_id) = layout.specialized_type_ids.get(key).copied() else {
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
        push(type_id, TypeKind::Enum, aux);
    }

    (TypeTable { records }, remap)
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
    pool.fill_from_ir(&ir.constants)?;
    let mut code = Vec::new();
    let mut records = Vec::new();
    let mut pc_spans = PcSpanTable::default();
    let file_id = if emit_pc_spans_enabled() {
        file_id_for_path(&mut pc_spans, None)
    } else {
        0
    };

    for func in &ir.functions {
        let offset = u32_section("code_offset", code.len())?;
        let emitted = emit::emit_function(
            func,
            &mut pool,
            &def_to_fn,
            &fn_arity,
            None,
            &typed.resolved,
        )?;
        if emit_pc_spans_enabled() {
            record_emitted_pc_spans(&mut pc_spans, func.id.index(), file_id, &emitted.pc_spans);
        }
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
            return_type_id: bytecode_return_type_id(func, typed),
        });
        code.extend_from_slice(&emitted.code);
    }

    let entry_function_id = ir
        .entry
        .and_then(|main| def_to_fn.get(&main).copied())
        .unwrap_or(ENTRY_NONE);

    let constants = pool.finish();
    let local_layouts = build_local_layouts(ir, typed);
    Ok(BytecodeModule {
        header: module_header(entry_function_id, &pc_spans),
        constants,
        types: build_type_table(layout, &typed.types),
        functions: FunctionTable { functions: records },
        code,
        local_layouts,
        pc_spans,
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
    source_file: Option<&str>,
) -> Result<BytecodeModule, CodegenError> {
    use error::u32_section;
    let layout = &typed.layout;
    let def_to_fn = global_fn;
    let fn_arity = build_fn_arity_map(typed, ir, global_fn);
    let used_types = collect_type_ids_from_ir(ir);
    let (module_types, type_remap) = build_module_type_table(layout, &typed.types, &used_types);

    let mut pool = ConstPoolBuilder::new();
    pool.fill_from_ir(&ir.constants)?;
    let mut code = Vec::new();
    let mut records = Vec::new();
    let mut pc_spans = PcSpanTable::default();
    let file_id = if emit_pc_spans_enabled() {
        file_id_for_path(&mut pc_spans, source_file)
    } else {
        0
    };

    for func in &ir.functions {
        let fn_id = global_fn.get(&func.def).copied().unwrap_or(func.id.index());
        let offset = u32_section("code_offset", code.len())?;
        let emitted = emit::emit_function(
            func,
            &mut pool,
            def_to_fn,
            &fn_arity,
            Some(&type_remap),
            &typed.resolved,
        )?;
        if emit_pc_spans_enabled() {
            record_emitted_pc_spans(&mut pc_spans, fn_id, file_id, &emitted.pc_spans);
        }
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
            return_type_id: bytecode_return_type_id(func, typed),
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
        header: module_header(entry_function_id, &pc_spans),
        constants,
        types: module_types,
        functions: FunctionTable { functions: records },
        code,
        local_layouts: LocalLayoutTable { layouts },
        pc_spans,
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

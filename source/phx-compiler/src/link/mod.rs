//! PHX0 linker — merge per-module object files into one executable image.

use phx_bytecode::{
    BytecodeModule, ConstPool, FileHeader, FunctionRecord, FunctionTable, Instruction,
    LocalLayoutTable, Opcode, TypeTable,
};
use std::collections::HashMap;

/// One module object to link.
#[derive(Debug, Clone)]
pub struct LinkInput {
    /// Logical module path (for diagnostics).
    pub logical_path: String,
    /// Object file.
    pub module: BytecodeModule,
}

/// Linker failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// No input modules.
    EmptyInput,
    /// `entry_function_id` not present after merge.
    InvalidEntry {
        /// Requested entry id.
        entry_id: u32,
    },
    /// Duplicate global function id across inputs.
    DuplicateFunctionId {
        /// Conflicting id.
        id: u32,
        /// First module path.
        first: String,
        /// Second module path.
        second: String,
    },
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => f.write_str("linker: no input modules"),
            Self::InvalidEntry { entry_id } => {
                write!(f, "linker: entry function id {entry_id} not found")
            }
            Self::DuplicateFunctionId { id, first, second } => {
                write!(
                    f,
                    "linker: duplicate function id {id} in `{first}` and `{second}`"
                )
            }
        }
    }
}

impl std::error::Error for LinkError {}

/// Merges `inputs` into one [`BytecodeModule`].
///
/// Function ids must be globally unique across inputs (use a global map when emitting per-module
/// objects). `entry_function_id` is the global id of `main`.
///
/// # Errors
///
/// Returns [`LinkError`] on duplicate ids or missing entry.
pub fn link_modules(
    inputs: &[LinkInput],
    entry_function_id: u32,
) -> Result<BytecodeModule, LinkError> {
    if inputs.is_empty() {
        return Err(LinkError::EmptyInput);
    }
    if inputs.len() == 1 {
        let mut m = inputs[0].module.clone();
        m.header.entry_function_id = entry_function_id;
        if entry_function_id == 0 {
            return Ok(m);
        }
        if m.functions
            .functions
            .iter()
            .any(|f| f.function_id == entry_function_id)
        {
            return Ok(m);
        }
        return Err(LinkError::InvalidEntry {
            entry_id: entry_function_id,
        });
    }

    let mut merged_constants = ConstPool::default();
    let mut merged_types = TypeTable::default();
    let mut merged_functions = Vec::new();
    let mut merged_code = Vec::new();
    let mut merged_layouts = LocalLayoutTable::default();

    let mut const_off = 0u32;
    let mut type_off = 0u32;
    let mut fn_id_seen: HashMap<u32, String> = HashMap::new();

    for input in inputs {
        let m = &input.module;
        let const_base = const_off;
        const_off += u32::try_from(m.constants.entries.len()).unwrap_or(u32::MAX);
        let type_base = type_off;
        type_off += u32::try_from(m.types.records.len()).unwrap_or(u32::MAX);

        for entry in &m.constants.entries {
            merged_constants.entries.push(entry.clone());
        }
        for rec in &m.types.records {
            let mut r = rec.clone();
            r.type_id = r.type_id.saturating_add(type_base);
            merged_types.records.push(r);
        }

        for f in &m.functions.functions {
            if let Some(prev) = fn_id_seen.insert(f.function_id, input.logical_path.clone()) {
                return Err(LinkError::DuplicateFunctionId {
                    id: f.function_id,
                    first: prev,
                    second: input.logical_path.clone(),
                });
            }
            let code_offset = u32::try_from(merged_code.len()).unwrap_or(u32::MAX);
            let body = slice_code(&m.code, f.code_offset, f.code_len);
            let patched = patch_code(body, const_base, type_base, f.function_id, &m.functions);
            let code_len = u32::try_from(patched.len()).unwrap_or(u32::MAX);
            merged_code.extend_from_slice(&patched);
            merged_functions.push(FunctionRecord {
                function_id: f.function_id,
                name_symbol_id: f.name_symbol_id,
                arity: f.arity,
                local_count: f.local_count,
                stack_max: f.stack_max,
                flags: f.flags,
                code_offset,
                code_len,
                return_type_id: f.return_type_id.saturating_add(type_base),
            });
        }

        merged_layouts
            .layouts
            .extend(m.local_layouts.layouts.iter().cloned());
    }

    if entry_function_id != 0
        && !merged_functions
            .iter()
            .any(|f| f.function_id == entry_function_id)
    {
        return Err(LinkError::InvalidEntry {
            entry_id: entry_function_id,
        });
    }

    Ok(BytecodeModule {
        header: FileHeader {
            entry_function_id,
            section_count: 5,
            ..FileHeader::new(5, entry_function_id)
        },
        constants: merged_constants,
        types: merged_types,
        functions: FunctionTable {
            functions: merged_functions,
        },
        code: merged_code,
        local_layouts: merged_layouts,
    })
}

fn slice_code(code: &[u8], offset: u32, len: u32) -> &[u8] {
    let start = usize::try_from(offset).unwrap_or(0);
    let end = start.saturating_add(usize::try_from(len).unwrap_or(0));
    if end <= code.len() {
        &code[start..end]
    } else {
        &[]
    }
}

fn patch_code(
    body: &[u8],
    const_base: u32,
    type_base: u32,
    _self_fn: u32,
    _functions: &FunctionTable,
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < body.len() {
        let Ok((inst, next)) = Instruction::decode_at(body, pos) else {
            break;
        };
        let patched = patch_instruction(&inst, const_base, type_base);
        out.extend(patched.encode());
        pos = next;
    }
    out
}

fn patch_instruction(inst: &Instruction, const_base: u32, type_base: u32) -> Instruction {
    let mut ops = inst.operands.clone();
    match inst.opcode {
        Opcode::Const if !ops.is_empty() => {
            ops[0] = ops[0].saturating_add(const_base);
        }
        Opcode::MakeStruct | Opcode::MakeEnum | Opcode::MakeArray | Opcode::MakeTuple
            if !ops.is_empty() => {
                ops[0] = ops[0].saturating_add(type_base);
            }
        _ => {}
    }
    Instruction {
        opcode: inst.opcode,
        operands: ops,
    }
}

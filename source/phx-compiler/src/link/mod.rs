//! PHX0 linker — merge per-module object files into one executable image.

use phx_bytecode::{
    BytecodeModule, ConstPool, ENTRY_NONE, FileHeader, FunctionRecord, FunctionTable, Instruction,
    LocalLayoutTable, TypeTable,
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
    /// Section or offset does not fit in `u32`.
    SectionTooLarge {
        /// Section name.
        section: &'static str,
        /// Length that overflowed.
        len: usize,
    },
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

fn u32_link(section: &'static str, len: usize) -> Result<u32, LinkError> {
    u32::try_from(len).map_err(|_| LinkError::SectionTooLarge { section, len })
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SectionTooLarge { section, len } => {
                write!(f, "linker: section `{section}` size {len} exceeds u32::MAX")
            }
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
/// ## Function ids and `Call` operands
///
/// Per-module codegen assigns **globally unique** `function_id` values before link (see
/// `build_global_fn_map` in the build driver). [`phx_bytecode::Opcode::Call`] and
/// [`phx_bytecode::Opcode::MakeFnPtr`] operands are those ids; the linker does **not** rewrite
/// them. It rebases constant-pool indices on [`phx_bytecode::Opcode::Const`] and
/// [`phx_bytecode::Opcode::MakeStr`], and type-table indices on
/// [`phx_bytecode::Opcode::MakeStruct`], [`phx_bytecode::Opcode::MakeEnum`],
/// [`phx_bytecode::Opcode::GetField`], [`phx_bytecode::Opcode::SetField`],
/// [`phx_bytecode::Opcode::MatchTag`], and the `sig_type_id` operand of
/// [`phx_bytecode::Opcode::CallIndirect`].
/// Duplicate `function_id` across inputs is [`LinkError::DuplicateFunctionId`].
///
/// `entry_function_id` is the global id of `main` (or [`ENTRY_NONE`] for libraries).
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
        if entry_function_id == ENTRY_NONE {
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
        const_off =
            const_off.saturating_add(u32_link("constants_count", m.constants.entries.len())?);
        let type_base = type_off;
        type_off = type_off.saturating_add(u32_link("types_count", m.types.records.len())?);

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
            let code_offset = u32_link("code_offset", merged_code.len())?;
            let body = slice_code(&m.code, f.code_offset, f.code_len);
            let patched = patch_code(body, const_base, type_base, f.function_id, &m.functions);
            let code_len = u32_link("code_len", patched.len())?;
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

    if entry_function_id != ENTRY_NONE
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
        let patched = inst.apply_link_bases(const_base, type_base);
        out.extend(patched.encode());
        pos = next;
    }
    out
}

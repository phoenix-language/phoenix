//! PHX0 linker — merge per-module object files into one executable image.
//!
//! Each compiled Phoenix module is a standalone [`BytecodeModule`] (constants, types, function
//! table, code section). The build driver assigns globally unique `function_id` values across the
//! workspace and path dependencies, then calls [`link_modules`] to concatenate sections and
//! rebase pool indices so cross-module calls resolve correctly.
//!
//! ## Pipeline position
//!
//! Runs after per-module codegen in the build driver ([`crate::build::driver::package`]) and
//! before PHX0 encode/verify. Workspace crates link local modules plus dependency `.phx0` objects
//! collected by [`crate::build::driver::link_map`].
//!
//! ## Merge strategy
//!
//! | Section | Merge rule |
//! | ------- | ---------- |
//! | Constants | Append; [`phx_bytecode::Opcode::Const`] / [`phx_bytecode::Opcode::MakeStr`] operands rebased by per-module offset |
//! | Types | Append; `type_id` and type-table operands rebased (see [`link_modules`]) |
//! | Functions | Append; `function_id` must be unique across inputs |
//! | Code | Append; each function body patched via [`phx_bytecode::Instruction::apply_link_bases`] |
//! | Local layouts / PC spans | Append / merge |
//!
//! [`phx_bytecode::Opcode::Call`] and [`phx_bytecode::Opcode::MakeFnPtr`] operands are **not**
//! rewritten — codegen assigns global ids before link (see `build_global_fn_map` in the build
//! driver).
//!
//! ## Public API
//!
//! [`LinkInput`] carries one object file; [`link_modules`] returns a single linked
//! [`BytecodeModule`]. Failures surface as [`LinkError`] and are wrapped by
//! [`crate::build::BuildError::Link`] during `phx build`.

use phx_bytecode::{
    BytecodeModule, ConstPool, ENTRY_NONE, FileHeader, FunctionRecord, FunctionTable, InstrError,
    Instruction, LocalLayoutTable, PHX0_HAS_DEBUG, PcSpanTable, TypeTable,
};
use std::collections::HashMap;

/// One module object file supplied to the linker.
///
/// The build driver sets [`logical_path`](Self::logical_path) for diagnostics (for example when
/// two inputs claim the same `function_id`). [`module`](Self::module) is the decoded PHX0 object
/// produced by [`crate::codegen::codegen`] for that compilation unit.
#[derive(Debug, Clone)]
pub struct LinkInput {
    /// Logical module path (for diagnostics).
    pub logical_path: String,
    /// Object file.
    pub module: BytecodeModule,
}

/// Failure while merging PHX0 object files.
///
/// Returned by [`link_modules`]. Embedders using [`crate::build::build_project`] receive these
/// as [`crate::build::BuildError::Link`].
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
    /// Malformed instruction while patching linked bytecode.
    Instruction(InstrError),
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
            Self::Instruction(err) => write!(f, "linker: {err}"),
        }
    }
}

impl std::error::Error for LinkError {}

/// Merges `inputs` into one [`BytecodeModule`].
///
/// Concatenates constant pools, type tables, function records, and code sections from each
/// [`LinkInput`], rebasing per-module indices so the linked image is self-consistent. Sets
/// [`phx_bytecode::FileHeader::entry_function_id`] on the result.
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
/// `entry_function_id` is the global id of `main` (or [`ENTRY_NONE`] for libraries). A single
/// input is returned unchanged (aside from the header entry field) when ids are already valid.
///
/// # Errors
///
/// Returns [`LinkError::EmptyInput`] when `inputs` is empty.
/// Returns [`LinkError::DuplicateFunctionId`] when two modules share a `function_id`.
/// Returns [`LinkError::InvalidEntry`] when `entry_function_id` is not [`ENTRY_NONE`] and no
/// merged function record matches.
/// Returns [`LinkError::SectionTooLarge`] when a section length exceeds `u32::MAX`.
/// Returns [`LinkError::Instruction`] when bytecode patching fails to decode an instruction.
///
/// # Panics
///
/// Never panics on malformed user bytecode — decode failures become [`LinkError::Instruction`].
#[allow(clippy::too_many_lines)]
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
    let mut merged_pc_spans = PcSpanTable::default();

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
            let patched = patch_code(body, const_base, type_base, f.function_id, &m.functions)
                .map_err(LinkError::Instruction)?;
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
                return_type_id: link_return_type_id(f.return_type_id, type_base),
            });
        }

        merged_layouts
            .layouts
            .extend(m.local_layouts.layouts.iter().cloned());
        merged_pc_spans.merge_from(m.pc_spans.clone());
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

    let section_count = u32::from(5u8.saturating_add(u8::from(!merged_pc_spans.is_empty())));
    let flags = if merged_pc_spans.is_empty() {
        0
    } else {
        PHX0_HAS_DEBUG
    };

    Ok(BytecodeModule {
        header: FileHeader {
            entry_function_id,
            section_count,
            flags,
            ..FileHeader::new(section_count, entry_function_id)
        },
        constants: merged_constants,
        types: merged_types,
        functions: FunctionTable {
            functions: merged_functions,
        },
        code: merged_code,
        local_layouts: merged_layouts,
        pc_spans: merged_pc_spans,
    })
}

/// Rebases a function's `return_type_id` when merging type tables across modules.
///
/// Codegen uses `0` and `1` as stack-depth sentinels (unit vs value); those must not
/// be shifted. Real layout type ids (`>= 2` in current codegen) pick up `type_base`.
fn link_return_type_id(return_type_id: u32, type_base: u32) -> u32 {
    if return_type_id <= 1 {
        return_type_id
    } else {
        return_type_id.saturating_add(type_base)
    }
}

/// Returns the function body slice from the module code section, or empty when out of bounds.
fn slice_code(code: &[u8], offset: u32, len: u32) -> &[u8] {
    let start = usize::try_from(offset).unwrap_or(0);
    let end = start.saturating_add(usize::try_from(len).unwrap_or(0));
    if end <= code.len() {
        &code[start..end]
    } else {
        &[]
    }
}

/// Decodes `body`, rebases pool operands, and re-encodes for the merged image.
fn patch_code(
    body: &[u8],
    const_base: u32,
    type_base: u32,
    _self_fn: u32,
    _functions: &FunctionTable,
) -> Result<Vec<u8>, InstrError> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < body.len() {
        let (inst, next) = Instruction::decode_at(body, pos)?;
        let patched = inst.apply_link_bases(const_base, type_base);
        out.extend(patched.encode()?);
        pos = next;
    }
    Ok(out)
}

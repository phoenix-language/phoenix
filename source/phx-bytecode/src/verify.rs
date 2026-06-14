//! Bytecode verifier — header, sections, control flow, and stack depth.

use std::collections::{HashMap, HashSet};

use super::cast::{PrimitiveKind, SLOT_KIND_AGG, SLOT_KIND_FN_PTR};
use super::const_pool::{ConstEntry, ConstTag};
use super::function::FunctionRecord;
use super::header::{HEADER_SIZE, HeaderError, MAGIC};
use super::instr::{InstrError, Instruction};
use super::local_layout::{FunctionLocalLayout, LocalLayoutTable};
use super::module::BytecodeModule;
use super::opcode::Opcode;
use super::section::{SectionEntry, SectionError, validate_section_table};
use super::stack_flow::{StackFlowError, analyze_stack_cfg, return_stack_depth};
use super::types::TypeKind;

/// Verifier failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// File too small for header.
    Truncated,
    /// Magic is not `PHX0`.
    BadMagic,
    /// Section table or payload extends past file end.
    SectionOutOfBounds,
    /// MVP flags field must be zero.
    NonZeroFlags,
    /// Header major/minor version is not supported.
    UnsupportedVersion {
        /// Major version read from header.
        major: u16,
        /// Minor version read from header.
        minor: u16,
    },
    /// In-memory `section_count` does not match the encoded file header.
    SectionCountMismatch {
        /// Value stored on [`BytecodeModule::header`].
        in_header: u32,
        /// Value read from re-encoded file bytes.
        in_file: u32,
    },
    /// Two section table rows share the same `section_kind`.
    DuplicateSectionKind {
        /// Duplicated section kind.
        kind: super::section::SectionKind,
    },
    /// Two section payloads overlap in byte range.
    OverlappingSections {
        /// First overlapping section kind.
        first: super::section::SectionKind,
        /// Second overlapping section kind.
        second: super::section::SectionKind,
    },
    /// `entry_function_id` missing or has non-zero arity.
    InvalidEntryFunction,
    /// Function `code_offset`/`code_len` out of code section bounds.
    FunctionCodeOutOfBounds {
        /// Offending function id.
        function_id: u32,
    },
    /// Instruction stream could not be decoded.
    MalformedInstruction {
        /// Function containing the bad bytecode.
        function_id: u32,
        /// Offset within the function body.
        offset: u32,
    },
    /// Jump target is not an instruction boundary in the function.
    InvalidJumpTarget {
        /// Function containing the jump.
        function_id: u32,
        /// Offset of the jump instruction.
        offset: u32,
        /// Invalid target offset.
        target: u32,
    },
    /// Local slot index >= `local_count`.
    LocalIndexOutOfRange {
        /// Function containing the instruction.
        function_id: u32,
        /// Local slot operand.
        slot: u32,
    },
    /// Constant pool index out of range.
    InvalidConstIndex {
        /// Function containing the instruction.
        function_id: u32,
        /// Constant pool operand.
        index: u32,
    },
    /// Callee function id not in the module.
    InvalidCallTarget {
        /// Function containing the call.
        function_id: u32,
        /// Callee function id operand.
        callee: u32,
    },
    /// Stack depth would go negative while simulating a function body.
    StackUnderflow {
        /// Function containing the instruction.
        function_id: u32,
        /// Offset of the offending instruction.
        offset: u32,
    },
    /// Control-flow predecessors require different stack depths at the same block entry.
    JoinDepthMismatch {
        /// Function containing the merge point.
        function_id: u32,
        /// Block entry offset where depths disagree.
        offset: u32,
        /// Depth already recorded from another predecessor.
        expected: u32,
        /// Depth from the conflicting predecessor.
        found: u32,
    },
    /// [`Opcode::Return`] reached with wrong operand-stack depth.
    ReturnDepthMismatch {
        /// Function containing the return.
        function_id: u32,
        /// Offset of the return instruction.
        offset: u32,
        /// Required depth from the function return type.
        expected: u32,
        /// Observed depth at the return.
        found: u32,
    },
    /// Simulated max stack depth exceeds declared `stack_max`.
    StackExceedsMax {
        /// Function whose body exceeds its limit.
        function_id: u32,
        /// Observed max depth.
        observed: u32,
        /// Declared limit.
        limit: u16,
    },
    /// Constant payload length does not match `prim_kind` on `Const`.
    ConstPayloadMismatch {
        /// Function containing the instruction.
        function_id: u32,
        /// Constant pool index.
        index: u32,
        /// `prim_kind` operand wire byte.
        prim_kind: u8,
        /// Expected payload length in bytes.
        expected_len: usize,
        /// Actual payload length in bytes.
        actual_len: usize,
    },
    /// Constant tag does not match signed/unsigned `prim_kind`.
    ConstTagMismatch {
        /// Function containing the instruction.
        function_id: u32,
        /// Constant pool index.
        index: u32,
    },
    /// Invalid `prim_kind` wire byte on an instruction operand.
    InvalidPrimKind {
        /// Function containing the instruction.
        function_id: u32,
        /// Offset within the function body.
        offset: u32,
        /// Invalid wire byte.
        prim_kind: u8,
    },
    /// Local layouts section present but no row for this function.
    MissingLocalLayout {
        /// Function id without a layout row.
        function_id: u32,
    },
    /// Layout `slot_count` does not match `FunctionRecord.local_count`.
    LocalLayoutMismatch {
        /// Function id.
        function_id: u32,
        /// Slots in layout section.
        layout_slots: usize,
        /// `local_count` in function record.
        local_count: u16,
    },
    /// `LoadLocal` / `StoreLocal` `prim_kind` does not match layout slot kind.
    LocalPrimKindMismatch {
        /// Function containing the instruction.
        function_id: u32,
        /// Local slot index.
        slot: u32,
        /// Operand `prim_kind` wire byte.
        prim_kind: u8,
    },
    /// `MakeFnPtr` `target_kind` is not `0` (Phoenix) or `1` (foreign).
    InvalidFnPtrTargetKind {
        /// Function containing the instruction.
        function_id: u32,
        /// Invalid `target_kind` operand.
        target_kind: u32,
    },
    /// `MakeFnPtr` Phoenix target references an unknown function id.
    InvalidFnPtrTarget {
        /// Function containing the instruction.
        function_id: u32,
        /// Invalid `target_id` operand.
        target_id: u32,
    },
    /// `CallIndirect` `sig_type_id` is missing or not `TypeKind::FnSig`.
    InvalidFnSigType {
        /// Function containing the instruction.
        function_id: u32,
        /// Invalid `sig_type_id` operand.
        sig_type_id: u32,
    },
    /// `CallIndirect` `expected_arity` does not match `FnSig` param count.
    CallIndirectArityMismatch {
        /// Function containing the instruction.
        function_id: u32,
        /// `expected_arity` operand.
        expected_arity: u32,
        /// Param count from type table aux.
        sig_param_count: u32,
    },
}

#[allow(clippy::too_many_lines)]
impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => write!(f, "truncated bytecode file"),
            Self::BadMagic => write!(f, "invalid magic (expected PHX0)"),
            Self::SectionOutOfBounds => write!(f, "section extends past file end"),
            Self::NonZeroFlags => write!(f, "non-zero header flags"),
            Self::UnsupportedVersion { major, minor } => {
                write!(f, "unsupported bytecode version {major}.{minor}")
            }
            Self::SectionCountMismatch { in_header, in_file } => write!(
                f,
                "section_count mismatch: header has {in_header}, file has {in_file}"
            ),
            Self::DuplicateSectionKind { kind } => {
                write!(f, "duplicate section kind {kind:?}")
            }
            Self::OverlappingSections { first, second } => {
                write!(f, "overlapping sections {first:?} and {second:?}")
            }
            Self::InvalidEntryFunction => {
                write!(f, "entry function missing or has non-zero arity")
            }
            Self::FunctionCodeOutOfBounds { function_id } => {
                write!(f, "function {function_id} code range out of bounds")
            }
            Self::MalformedInstruction {
                function_id,
                offset,
            } => {
                write!(
                    f,
                    "malformed instruction in function {function_id} at offset {offset}"
                )
            }
            Self::InvalidJumpTarget {
                function_id,
                offset,
                target,
            } => write!(
                f,
                "invalid jump target {target} in function {function_id} at offset {offset}"
            ),
            Self::LocalIndexOutOfRange { function_id, slot } => {
                write!(
                    f,
                    "local slot {slot} out of range in function {function_id}"
                )
            }
            Self::InvalidConstIndex { function_id, index } => {
                write!(
                    f,
                    "constant index {index} out of range in function {function_id}"
                )
            }
            Self::InvalidCallTarget {
                function_id,
                callee,
            } => {
                write!(f, "call target {callee} invalid in function {function_id}")
            }
            Self::StackUnderflow {
                function_id,
                offset,
            } => {
                write!(
                    f,
                    "stack underflow in function {function_id} at offset {offset}"
                )
            }
            Self::JoinDepthMismatch {
                function_id,
                offset,
                expected,
                found,
            } => write!(
                f,
                "join stack depth mismatch in function {function_id} at offset {offset}: expected {expected}, found {found}"
            ),
            Self::ReturnDepthMismatch {
                function_id,
                offset,
                expected,
                found,
            } => write!(
                f,
                "return stack depth mismatch in function {function_id} at offset {offset}: expected {expected}, found {found}"
            ),
            Self::StackExceedsMax {
                function_id,
                observed,
                limit,
            } => write!(
                f,
                "function {function_id} stack depth {observed} exceeds stack_max {limit}"
            ),
            Self::ConstPayloadMismatch {
                function_id,
                index,
                prim_kind,
                expected_len,
                actual_len,
            } => write!(
                f,
                "constant {index} payload length {actual_len} != expected {expected_len} for prim_kind {prim_kind} in function {function_id}"
            ),
            Self::ConstTagMismatch { function_id, index } => write!(
                f,
                "constant {index} tag does not match prim_kind in function {function_id}"
            ),
            Self::InvalidPrimKind {
                function_id,
                offset,
                prim_kind,
            } => write!(
                f,
                "invalid prim_kind {prim_kind} in function {function_id} at offset {offset}"
            ),
            Self::MissingLocalLayout { function_id } => {
                write!(f, "missing local layout for function {function_id}")
            }
            Self::LocalLayoutMismatch {
                function_id,
                layout_slots,
                local_count,
            } => write!(
                f,
                "function {function_id} layout has {layout_slots} slots but local_count is {local_count}"
            ),
            Self::LocalPrimKindMismatch {
                function_id,
                slot,
                prim_kind,
            } => write!(
                f,
                "local slot {slot} prim_kind {prim_kind} mismatch in function {function_id}"
            ),
            Self::InvalidFnPtrTargetKind {
                function_id,
                target_kind,
            } => write!(
                f,
                "invalid fn ptr target_kind {target_kind} in function {function_id}"
            ),
            Self::InvalidFnPtrTarget {
                function_id,
                target_id,
            } => write!(
                f,
                "invalid fn ptr target_id {target_id} in function {function_id}"
            ),
            Self::InvalidFnSigType {
                function_id,
                sig_type_id,
            } => write!(
                f,
                "invalid fn sig type id {sig_type_id} in function {function_id}"
            ),
            Self::CallIndirectArityMismatch {
                function_id,
                expected_arity,
                sig_param_count,
            } => write!(
                f,
                "call indirect arity {expected_arity} != sig param count {sig_param_count} in function {function_id}"
            ),
        }
    }
}

impl std::error::Error for VerifyError {}

/// Verifies `module` invariants required before execution (MVP subset).
///
/// # Errors
///
/// Returns [`VerifyError`] when layout, control flow, or stack limits are invalid.
pub fn verify(module: &BytecodeModule) -> Result<(), VerifyError> {
    verify_header_and_sections(module)?;
    verify_entry_function(module)?;
    let fn_arity = function_arity_map(module);
    for func in &module.functions.functions {
        verify_function_body(func, module, &fn_arity)?;
    }
    Ok(())
}

fn verify_header_and_sections(module: &BytecodeModule) -> Result<(), VerifyError> {
    module.header.validate_version().map_err(|err| match err {
        HeaderError::UnsupportedVersion { major, minor } => {
            VerifyError::UnsupportedVersion { major, minor }
        }
        HeaderError::BadMagic => VerifyError::BadMagic,
    })?;

    let bytes = module
        .encode()
        .map_err(|_| VerifyError::SectionOutOfBounds)?;
    if bytes.len() < HEADER_SIZE {
        return Err(VerifyError::Truncated);
    }
    if bytes[0..4] != MAGIC {
        return Err(VerifyError::BadMagic);
    }
    if module.header.flags != 0 {
        return Err(VerifyError::NonZeroFlags);
    }

    let encoded_section_count = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    if module.header.section_count != encoded_section_count {
        return Err(VerifyError::SectionCountMismatch {
            in_header: module.header.section_count,
            in_file: encoded_section_count,
        });
    }

    let table_end = HEADER_SIZE.saturating_add(
        usize::try_from(encoded_section_count)
            .unwrap_or(0)
            .saturating_mul(12),
    );
    if bytes.len() < table_end {
        return Err(VerifyError::Truncated);
    }

    let mut entries = Vec::with_capacity(usize::try_from(encoded_section_count).unwrap_or(0));
    for i in 0..encoded_section_count {
        let start = HEADER_SIZE + usize::try_from(i).unwrap_or(0).saturating_mul(12);
        let entry_bytes: &[u8; 12] = bytes[start..start + 12]
            .try_into()
            .map_err(|_| VerifyError::Truncated)?;
        entries.push(SectionEntry::decode(entry_bytes).map_err(section_error_to_verify)?);
    }
    validate_section_table(&entries, bytes.len()).map_err(section_error_to_verify)?;
    Ok(())
}

fn section_error_to_verify(err: SectionError) -> VerifyError {
    match err {
        SectionError::OutOfBounds | SectionError::UnknownKind(_) => VerifyError::SectionOutOfBounds,
        SectionError::DuplicateKind(kind) => VerifyError::DuplicateSectionKind { kind },
        SectionError::OverlappingSections { first, second } => {
            VerifyError::OverlappingSections { first, second }
        }
    }
}

fn verify_entry_function(module: &BytecodeModule) -> Result<(), VerifyError> {
    let entry_id = module.header.entry_function_id;
    if entry_id == crate::header::ENTRY_NONE {
        return Ok(());
    }
    let entry = module
        .functions
        .functions
        .iter()
        .find(|f| f.function_id == entry_id)
        .ok_or(VerifyError::InvalidEntryFunction)?;
    if entry.arity != 0 {
        return Err(VerifyError::InvalidEntryFunction);
    }
    Ok(())
}

fn function_arity_map(module: &BytecodeModule) -> HashMap<u32, u16> {
    module
        .functions
        .functions
        .iter()
        .map(|f| (f.function_id, f.arity))
        .collect()
}

fn function_code<'a>(module: &'a BytecodeModule, func: &FunctionRecord) -> Option<&'a [u8]> {
    let start = usize::try_from(func.code_offset).ok()?;
    let end = start.checked_add(usize::try_from(func.code_len).ok()?)?;
    module.code.get(start..end)
}

fn verify_function_body(
    func: &FunctionRecord,
    module: &BytecodeModule,
    fn_arity: &HashMap<u32, u16>,
) -> Result<(), VerifyError> {
    verify_function_layout(func, &module.local_layouts, module.header.version_minor)?;

    let code = function_code(module, func).ok_or(VerifyError::FunctionCodeOutOfBounds {
        function_id: func.function_id,
    })?;
    if code.len() != usize::try_from(func.code_len).unwrap_or(usize::MAX) {
        return Err(VerifyError::FunctionCodeOutOfBounds {
            function_id: func.function_id,
        });
    }

    let mut inst_starts = HashSet::new();
    let mut instructions = Vec::new();
    let mut offset = 0usize;
    while offset < code.len() {
        let rel = u32::try_from(offset).unwrap_or(u32::MAX);
        inst_starts.insert(rel);
        match Instruction::decode_at(code, offset) {
            Ok((inst, next)) => {
                instructions.push((rel, inst));
                offset = next;
            }
            Err(InstrError::Truncated | InstrError::Opcode(_)) => {
                return Err(VerifyError::MalformedInstruction {
                    function_id: func.function_id,
                    offset: rel,
                });
            }
        }
    }

    let code_len = func.code_len;
    let layout = module.local_layouts.for_function(func.function_id);
    for (rel, inst) in &instructions {
        verify_operands(
            func,
            *rel,
            inst,
            &inst_starts,
            code_len,
            fn_arity,
            module,
            layout,
        )?;
    }

    let return_depth = return_stack_depth(func.return_type_id, &module.types);
    let summary = analyze_stack_cfg(&instructions, &inst_starts, fn_arity, return_depth).map_err(
        |e| match e {
            StackFlowError::Underflow { offset } => VerifyError::StackUnderflow {
                function_id: func.function_id,
                offset,
            },
            StackFlowError::JoinDepthMismatch {
                offset,
                expected,
                found,
            } => VerifyError::JoinDepthMismatch {
                function_id: func.function_id,
                offset,
                expected,
                found,
            },
            StackFlowError::ReturnDepthMismatch {
                offset,
                expected,
                found,
            } => VerifyError::ReturnDepthMismatch {
                function_id: func.function_id,
                offset,
                expected,
                found,
            },
            StackFlowError::InvalidCallTarget { callee, .. } => VerifyError::InvalidCallTarget {
                function_id: func.function_id,
                callee,
            },
        },
    )?;
    let max_depth = summary.max_depth;

    if max_depth > u32::from(func.stack_max) {
        return Err(VerifyError::StackExceedsMax {
            function_id: func.function_id,
            observed: max_depth,
            limit: func.stack_max,
        });
    }

    Ok(())
}

fn verify_function_layout(
    func: &FunctionRecord,
    layouts: &LocalLayoutTable,
    version_minor: u16,
) -> Result<(), VerifyError> {
    if version_minor < 1 || func.local_count == 0 {
        return Ok(());
    }
    let Some(layout) = layouts.for_function(func.function_id) else {
        return Err(VerifyError::MissingLocalLayout {
            function_id: func.function_id,
        });
    };
    if layout.slots.len() != usize::from(func.local_count) {
        return Err(VerifyError::LocalLayoutMismatch {
            function_id: func.function_id,
            layout_slots: layout.slots.len(),
            local_count: func.local_count,
        });
    }
    Ok(())
}

fn prim_kind_operand(raw: u32) -> Option<u8> {
    u8::try_from(raw).ok()
}

fn verify_const_entry(
    function_id: u32,
    index: u32,
    prim_kind_byte: u8,
    entry: &ConstEntry,
) -> Result<(), VerifyError> {
    let Some(prim) = PrimitiveKind::from_u8(prim_kind_byte) else {
        return Err(VerifyError::InvalidPrimKind {
            function_id,
            offset: 0,
            prim_kind: prim_kind_byte,
        });
    };

    let expected_len = match entry.tag {
        ConstTag::Bool => 1usize,
        ConstTag::Float32 => 4,
        ConstTag::Float64 => 8,
        ConstTag::Bytes => return Ok(()),
        ConstTag::SignedInt | ConstTag::UnsignedInt => usize::from(prim.byte_size()),
    };

    let actual_len = entry.payload.len();
    if actual_len != expected_len {
        return Err(VerifyError::ConstPayloadMismatch {
            function_id,
            index,
            prim_kind: prim_kind_byte,
            expected_len,
            actual_len,
        });
    }

    let tag_ok = match entry.tag {
        ConstTag::SignedInt => prim.is_signed_int(),
        ConstTag::UnsignedInt => prim.is_unsigned_int(),
        ConstTag::Bool => prim == PrimitiveKind::Bool,
        ConstTag::Float32 => prim == PrimitiveKind::F32,
        ConstTag::Float64 => prim == PrimitiveKind::F64,
        ConstTag::Bytes => true,
    };
    if !tag_ok {
        return Err(VerifyError::ConstTagMismatch { function_id, index });
    }
    Ok(())
}

fn check_local_slot(
    function_id: u32,
    slot: u32,
    prim_kind_byte: u8,
    func: &FunctionRecord,
    layout: Option<&FunctionLocalLayout>,
) -> Result<(), VerifyError> {
    if slot >= u32::from(func.local_count) {
        return Err(VerifyError::LocalIndexOutOfRange { function_id, slot });
    }
    let Some(layout) = layout else {
        return Ok(());
    };
    let Some(slot_kind) = layout.slots.get(slot as usize) else {
        return Err(VerifyError::LocalIndexOutOfRange { function_id, slot });
    };
    if slot_kind.is_aggregate() {
        if prim_kind_byte != SLOT_KIND_AGG {
            return Err(VerifyError::LocalPrimKindMismatch {
                function_id,
                slot,
                prim_kind: prim_kind_byte,
            });
        }
        return Ok(());
    }
    if slot_kind.is_fn_ptr() {
        if prim_kind_byte != SLOT_KIND_FN_PTR {
            return Err(VerifyError::LocalPrimKindMismatch {
                function_id,
                slot,
                prim_kind: prim_kind_byte,
            });
        }
        return Ok(());
    }
    let Some(expected) = slot_kind.primitive_kind() else {
        return Ok(());
    };
    if prim_kind_byte != expected.as_u8() {
        return Err(VerifyError::LocalPrimKindMismatch {
            function_id,
            slot,
            prim_kind: prim_kind_byte,
        });
    }
    Ok(())
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn verify_operands(
    func: &FunctionRecord,
    offset: u32,
    inst: &Instruction,
    inst_starts: &HashSet<u32>,
    code_len: u32,
    fn_arity: &HashMap<u32, u16>,
    module: &BytecodeModule,
    layout: Option<&FunctionLocalLayout>,
) -> Result<(), VerifyError> {
    let const_count = u32::try_from(module.constants.entries.len()).unwrap_or(u32::MAX);
    let function_id = func.function_id;
    match inst.opcode {
        Opcode::Const => {
            if inst.operands.len() != 2 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
            let index = inst.operands.first().copied().unwrap_or(0);
            if index >= const_count {
                return Err(VerifyError::InvalidConstIndex { function_id, index });
            }
            let Some(prim_kind_byte) =
                prim_kind_operand(inst.operands.get(1).copied().unwrap_or(0))
            else {
                return Err(VerifyError::InvalidPrimKind {
                    function_id,
                    offset,
                    prim_kind: 0xFF,
                });
            };
            if PrimitiveKind::from_u8(prim_kind_byte).is_none() {
                return Err(VerifyError::InvalidPrimKind {
                    function_id,
                    offset,
                    prim_kind: prim_kind_byte,
                });
            }
            let entry = &module.constants.entries[index as usize];
            verify_const_entry(function_id, index, prim_kind_byte, entry)?;
        }
        Opcode::LoadLocal | Opcode::StoreLocal => {
            if inst.operands.len() != 2 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
            let slot = inst.operands.first().copied().unwrap_or(0);
            let Some(prim_kind_byte) =
                prim_kind_operand(inst.operands.get(1).copied().unwrap_or(0))
            else {
                return Err(VerifyError::InvalidPrimKind {
                    function_id,
                    offset,
                    prim_kind: 0xFF,
                });
            };
            if prim_kind_byte != SLOT_KIND_AGG
                && prim_kind_byte != SLOT_KIND_FN_PTR
                && PrimitiveKind::from_u8(prim_kind_byte).is_none()
            {
                return Err(VerifyError::InvalidPrimKind {
                    function_id,
                    offset,
                    prim_kind: prim_kind_byte,
                });
            }
            check_local_slot(function_id, slot, prim_kind_byte, func, layout)?;
        }
        Opcode::Call => {
            if inst.operands.len() != 1 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
            let callee = inst.operands.first().copied().unwrap_or(0);
            if !fn_arity.contains_key(&callee) {
                return Err(VerifyError::InvalidCallTarget {
                    function_id,
                    callee,
                });
            }
        }
        Opcode::Jump | Opcode::JumpIfTrue | Opcode::JumpIfFalse => {
            if inst.operands.len() != 1 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
            let target = inst.operands[0];
            if target >= code_len || !inst_starts.contains(&target) {
                return Err(VerifyError::InvalidJumpTarget {
                    function_id,
                    offset,
                    target,
                });
            }
        }
        Opcode::Add
        | Opcode::Sub
        | Opcode::Mul
        | Opcode::Div
        | Opcode::Mod
        | Opcode::Pow
        | Opcode::Eq
        | Opcode::Lt
        | Opcode::Ne
        | Opcode::Le
        | Opcode::Ge
        | Opcode::BitAnd
        | Opcode::BitOr
        | Opcode::BitXor
        | Opcode::Shl
        | Opcode::Shr
        | Opcode::Neg
        | Opcode::Not
        | Opcode::BitNot
        | Opcode::MakeTuple
        | Opcode::MakeArray
        | Opcode::MakeSlice
        | Opcode::MakeSliceFromPtr => {
            if inst.operands.len() != 1 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
        }
        Opcode::MakeStr => {
            if inst.operands.len() != 1 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
            let index = inst.operands.first().copied().unwrap_or(0);
            if index >= const_count {
                return Err(VerifyError::InvalidConstIndex { function_id, index });
            }
            let entry = &module.constants.entries[index as usize];
            if entry.tag != ConstTag::Bytes {
                return Err(VerifyError::ConstTagMismatch { function_id, index });
            }
        }
        Opcode::StrAsSlice
        | Opcode::Index
        | Opcode::Pop
        | Opcode::Return
        | Opcode::Trap
        | Opcode::LoadAggViaLocalPtr
        | Opcode::Alloc
        | Opcode::Free => {
            if !inst.operands.is_empty() {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
        }
        Opcode::Cast
        | Opcode::MakeStruct
        | Opcode::GetField
        | Opcode::SetField
        | Opcode::MatchTag
        | Opcode::PtrLoad
        | Opcode::PtrStore
        | Opcode::IndexStore => {
            if inst.operands.len() != 2 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
        }
        Opcode::MakeEnum => {
            if inst.operands.len() != 3 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
        }
        Opcode::AddressOfLocal => {
            if inst.operands.len() != 1 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
            let slot = inst.operands.first().copied().unwrap_or(0);
            if slot >= u32::from(func.local_count) {
                return Err(VerifyError::LocalIndexOutOfRange { function_id, slot });
            }
        }
        Opcode::MakeFnPtr => {
            if inst.operands.len() != 2 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
            let target_kind = inst.operands.first().copied().unwrap_or(0);
            let target_id = inst.operands.get(1).copied().unwrap_or(0);
            if target_kind > 1 {
                return Err(VerifyError::InvalidFnPtrTargetKind {
                    function_id,
                    target_kind,
                });
            }
            if target_kind == 0 && !fn_arity.contains_key(&target_id) {
                return Err(VerifyError::InvalidFnPtrTarget {
                    function_id,
                    target_id,
                });
            }
        }
        Opcode::CallIndirect => {
            if inst.operands.len() != 2 {
                return Err(VerifyError::MalformedInstruction {
                    function_id,
                    offset,
                });
            }
            let expected_arity = inst.operands.first().copied().unwrap_or(0);
            let sig_type_id = inst.operands.get(1).copied().unwrap_or(0);
            let Some(record) = module
                .types
                .records
                .iter()
                .find(|r| r.type_id == sig_type_id)
            else {
                return Err(VerifyError::InvalidFnSigType {
                    function_id,
                    sig_type_id,
                });
            };
            if record.kind != TypeKind::FnSig {
                return Err(VerifyError::InvalidFnSigType {
                    function_id,
                    sig_type_id,
                });
            }
            let sig_param_count = fn_sig_param_count(&record.aux);
            if sig_param_count != expected_arity {
                return Err(VerifyError::CallIndirectArityMismatch {
                    function_id,
                    expected_arity,
                    sig_param_count,
                });
            }
        }
    }
    Ok(())
}

/// Reads `param_count` from a `TypeKind::FnSig` aux payload (`u32` LE).
fn fn_sig_param_count(aux: &[u8]) -> u32 {
    if aux.len() < 4 {
        return 0;
    }
    u32::from_le_bytes([aux[0], aux[1], aux[2], aux[3]])
}

#[cfg(test)]
#[allow(clippy::cast_lossless, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{
        ConstEntry, ConstPool, ConstTag, FileHeader, FunctionLocalLayout, FunctionRecord,
        FunctionTable, Instruction, LocalLayoutTable, Opcode, PrimitiveKind, TypeKind, TypeRecord,
        TypeTable,
    };

    fn minimal_module(
        code: Vec<u8>,
        stack_max: u16,
        entry_arity: u16,
        return_type_id: u32,
    ) -> BytecodeModule {
        BytecodeModule {
            header: FileHeader::new(5, 0),
            constants: ConstPool {
                entries: vec![ConstEntry {
                    tag: ConstTag::SignedInt,
                    payload: 1i32.to_le_bytes().to_vec(),
                }],
            },
            types: TypeTable::default(),
            functions: FunctionTable {
                functions: vec![FunctionRecord {
                    function_id: 0,
                    name_symbol_id: 0,
                    arity: entry_arity,
                    local_count: 0,
                    stack_max,
                    flags: 0,
                    code_offset: 0,
                    code_len: u32::try_from(code.len()).unwrap_or(0),
                    return_type_id,
                }],
            },
            code,
            local_layouts: LocalLayoutTable::default(),
        }
    }

    fn const_return_code() -> Vec<u8> {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0, PrimitiveKind::S32.as_u8() as u32],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );
        code
    }

    #[test]
    fn valid_const_return_passes() {
        let module = minimal_module(const_return_code(), 4, 0, 1);
        verify(&module).expect("verify");
    }

    #[test]
    fn reject_invalid_entry_arity() {
        let module = minimal_module(const_return_code(), 4, 1, 1);
        let err = verify(&module).unwrap_err();
        assert_eq!(err, VerifyError::InvalidEntryFunction);
    }

    #[test]
    fn reject_invalid_jump_target() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0, PrimitiveKind::S32.as_u8() as u32],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::JumpIfTrue,
                operands: vec![99],
            }
            .encode(),
        );
        let module = minimal_module(code, 4, 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::InvalidJumpTarget { function_id: 0, .. }
        ));
    }

    #[test]
    fn reject_zero_operand_jump() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Jump,
                operands: vec![],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );
        let module = minimal_module(code, 4, 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::MalformedInstruction { function_id: 0, .. }
        ));
    }

    #[test]
    fn reject_zero_operand_jump_if_true() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::JumpIfTrue,
                operands: vec![],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );
        let module = minimal_module(code, 4, 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::MalformedInstruction { function_id: 0, .. }
        ));
    }

    #[test]
    fn reject_zero_operand_jump_if_false() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::JumpIfFalse,
                operands: vec![],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );
        let module = minimal_module(code, 4, 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::MalformedInstruction { function_id: 0, .. }
        ));
    }

    #[test]
    fn reject_extra_operand_jump() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Jump,
                operands: vec![0, 1],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );
        let module = minimal_module(code, 4, 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::MalformedInstruction { function_id: 0, .. }
        ));
    }

    #[test]
    fn reject_zero_operand_call() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Call,
                operands: vec![],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );
        let module = minimal_module(code, 8, 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::MalformedInstruction { function_id: 0, .. }
        ));
    }

    #[test]
    fn reject_local_out_of_range() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::LoadLocal,
                operands: vec![0, PrimitiveKind::S32.as_u8() as u32],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );
        let module = minimal_module(code, 4, 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::LocalIndexOutOfRange { slot: 0, .. }
        ));
    }

    #[test]
    fn reject_stack_exceeds_max() {
        let module = minimal_module(const_return_code(), 0, 0, 1);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::StackExceedsMax {
                observed: 1,
                limit: 0,
                ..
            }
        ));
    }

    #[test]
    fn reject_stack_underflow() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Add,
                operands: vec![PrimitiveKind::S32.as_u8() as u32],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );
        let module = minimal_module(code, 4, 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::StackUnderflow { function_id: 0, .. }
        ));
    }

    #[test]
    fn reject_return_depth_mismatch() {
        let module = minimal_module(const_return_code(), 4, 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::ReturnDepthMismatch {
                function_id: 0,
                expected: 0,
                found: 1,
                ..
            }
        ));
    }

    #[test]
    fn reject_const_payload_width_mismatch() {
        let mut module = minimal_module(const_return_code(), 4, 0, 1);
        module.constants.entries[0].payload = 1i64.to_le_bytes().to_vec();
        let err = verify(&module).unwrap_err();
        assert!(matches!(err, VerifyError::ConstPayloadMismatch { .. }));
    }

    #[test]
    fn reject_invalid_call_target_in_stack_flow() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Call,
                operands: vec![99],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );
        let module = minimal_module(code, 8, 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::InvalidCallTarget { callee: 99, .. }
        ));
    }

    #[test]
    fn reject_malformed_cast_operands() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0, PrimitiveKind::S32.as_u8() as u32],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Cast,
                operands: vec![PrimitiveKind::S32.as_u8() as u32],
            }
            .encode(),
        );
        let module = minimal_module(code, 4, 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::MalformedInstruction { function_id: 0, .. }
        ));
    }

    fn module_with_types(code: Vec<u8>, types: TypeTable) -> BytecodeModule {
        use crate::LocalSlotKind;

        BytecodeModule {
            header: FileHeader::new(5, 0),
            constants: ConstPool {
                entries: vec![ConstEntry {
                    tag: ConstTag::SignedInt,
                    payload: 1i32.to_le_bytes().to_vec(),
                }],
            },
            types,
            functions: FunctionTable {
                functions: vec![
                    FunctionRecord {
                        function_id: 0,
                        name_symbol_id: 0,
                        arity: 0,
                        local_count: 0,
                        stack_max: 8,
                        flags: 0,
                        code_offset: 0,
                        code_len: u32::try_from(code.len()).unwrap_or(0),
                        return_type_id: 1,
                    },
                    FunctionRecord {
                        function_id: 1,
                        name_symbol_id: 0,
                        arity: 1,
                        local_count: 1,
                        stack_max: 8,
                        flags: 0,
                        code_offset: 0,
                        code_len: 0,
                        return_type_id: 0,
                    },
                ],
            },
            code,
            local_layouts: LocalLayoutTable {
                layouts: vec![FunctionLocalLayout {
                    function_id: 1,
                    slots: vec![LocalSlotKind::primitive(PrimitiveKind::S32)],
                }],
            },
        }
    }

    #[test]
    fn valid_make_fn_ptr_and_call_indirect_passes() {
        let sig_type = TypeRecord {
            type_id: 10,
            kind: TypeKind::FnSig,
            aux: 1u32.to_le_bytes().to_vec(),
        };
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::MakeFnPtr,
                operands: vec![0, 1],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0, PrimitiveKind::S32.as_u8() as u32],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::CallIndirect,
                operands: vec![1, 10],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );
        let module = module_with_types(
            code,
            TypeTable {
                records: vec![sig_type],
            },
        );
        verify(&module).expect("verify indirect call");
    }

    #[test]
    fn reject_call_indirect_arity_mismatch() {
        let sig_type = TypeRecord {
            type_id: 10,
            kind: TypeKind::FnSig,
            aux: 2u32.to_le_bytes().to_vec(),
        };
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::MakeFnPtr,
                operands: vec![0, 1],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::CallIndirect,
                operands: vec![1, 10],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );
        let module = module_with_types(
            code,
            TypeTable {
                records: vec![sig_type],
            },
        );
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::CallIndirectArityMismatch {
                expected_arity: 1,
                sig_param_count: 2,
                ..
            }
        ));
    }

    fn join_depth_mismatch_code() -> (Vec<u8>, u32) {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Const,
                operands: vec![0, PrimitiveKind::Bool.as_u8() as u32],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::JumpIfTrue,
                operands: vec![0],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Jump,
                operands: vec![0],
            }
            .encode(),
        );
        let then_off = u32::try_from(code.len()).unwrap_or(0);
        code.extend(
            Instruction {
                opcode: Opcode::Const,
                operands: vec![1, PrimitiveKind::S32.as_u8() as u32],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Jump,
                operands: vec![0],
            }
            .encode(),
        );
        let merge_off = u32::try_from(code.len()).unwrap_or(0);
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );

        let patch_operand = |code: &mut Vec<u8>, inst_offset: usize, target: u32| {
            let start = inst_offset + 2;
            code[start..start + 4].copy_from_slice(&target.to_le_bytes());
        };
        patch_operand(&mut code, 10, then_off);
        patch_operand(&mut code, 16, merge_off);
        patch_operand(
            &mut code,
            usize::try_from(then_off).unwrap_or(0) + 10,
            merge_off,
        );
        (code, merge_off)
    }

    #[test]
    fn reject_join_depth_mismatch() {
        let (code, merge_off) = join_depth_mismatch_code();
        let mut module = minimal_module(code, 4, 0, 0);
        module.constants.entries = vec![
            ConstEntry {
                tag: ConstTag::Bool,
                payload: vec![1],
            },
            ConstEntry {
                tag: ConstTag::SignedInt,
                payload: 1i32.to_le_bytes().to_vec(),
            },
        ];
        let err = verify(&module).unwrap_err();
        assert!(
            matches!(
                err,
                VerifyError::JoinDepthMismatch {
                    function_id: 0,
                    offset,
                    expected: 0,
                    found: 1,
                } if offset == merge_off
            ),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn reject_trap_with_operands() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Trap,
                operands: vec![0],
            }
            .encode(),
        );
        let module = minimal_module(code, 4, 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::MalformedInstruction { function_id: 0, .. }
        ));
    }

    #[test]
    fn valid_trap_without_operands_passes() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::Trap,
                operands: vec![],
            }
            .encode(),
        );
        let module = minimal_module(code, 4, 0, 0);
        verify(&module).expect("trap without operands");
    }

    #[test]
    fn reject_make_str_non_bytes_const() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::MakeStr,
                operands: vec![0],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );
        let module = minimal_module(code, 4, 0, 0);
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::ConstTagMismatch {
                function_id: 0,
                index: 0,
            }
        ));
    }

    #[test]
    fn reject_missing_local_layout_when_required() {
        let mut module = minimal_module(const_return_code(), 4, 0, 1);
        module.functions.functions[0].local_count = 1;
        let err = verify(&module).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::MissingLocalLayout { function_id: 0 }
        ));
    }

    #[test]
    fn valid_make_str_bytes_const_passes() {
        let mut code = Vec::new();
        code.extend(
            Instruction {
                opcode: Opcode::MakeStr,
                operands: vec![0],
            }
            .encode(),
        );
        code.extend(
            Instruction {
                opcode: Opcode::Return,
                operands: vec![],
            }
            .encode(),
        );
        let mut module = minimal_module(code, 4, 0, 1);
        module.constants.entries[0] = ConstEntry {
            tag: ConstTag::Bytes,
            payload: b"hi".to_vec(),
        };
        verify(&module).expect("make str bytes const");
    }

    #[test]
    fn reject_unsupported_version_major() {
        let mut module = minimal_module(const_return_code(), 4, 0, 1);
        module.header.version_major = 99;
        let err = verify(&module).unwrap_err();
        assert_eq!(
            err,
            VerifyError::UnsupportedVersion {
                major: 99,
                minor: crate::VERSION_MINOR,
            }
        );
    }

    #[test]
    fn reject_unsupported_version_minor() {
        let mut module = minimal_module(const_return_code(), 4, 0, 1);
        module.header.version_minor = crate::VERSION_MINOR + 1;
        let err = verify(&module).unwrap_err();
        assert_eq!(
            err,
            VerifyError::UnsupportedVersion {
                major: crate::VERSION_MAJOR,
                minor: crate::VERSION_MINOR + 1,
            }
        );
    }

    #[test]
    fn reject_section_count_mismatch() {
        let mut module = minimal_module(const_return_code(), 4, 0, 1);
        module.header.section_count = 3;
        let err = verify(&module).unwrap_err();
        assert_eq!(
            err,
            VerifyError::SectionCountMismatch {
                in_header: 3,
                in_file: 5,
            }
        );
    }
}

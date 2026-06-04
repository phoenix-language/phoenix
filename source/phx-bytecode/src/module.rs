//! PHX0 [`BytecodeModule`] aggregate and file encode/decode.

use super::const_pool::ConstPool;
use super::encode::{EncodeError, u32_len};
use super::function::FunctionTable;
use super::header::{FileHeader, HEADER_SIZE, HeaderError};
use super::instr::Instruction;
use super::local_layout::{LocalLayoutError, LocalLayoutTable};
use super::section::{SectionEntry, SectionError, SectionKind};
use super::types::TypeTable;

/// Decoded Phoenix bytecode module (MVP).
#[derive(Debug, Clone, PartialEq)]
pub struct BytecodeModule {
    /// File header.
    pub header: FileHeader,
    /// Constants section.
    pub constants: ConstPool,
    /// Types section.
    pub types: TypeTable,
    /// Function metadata.
    pub functions: FunctionTable,
    /// Raw code section bytes (instruction stream).
    pub code: Vec<u8>,
    /// Per-function local slot layout metadata.
    pub local_layouts: LocalLayoutTable,
}

impl BytecodeModule {
    /// Creates an empty MVP module with `main` as entry function id 0.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            header: FileHeader::new(5, 0),
            constants: ConstPool::default(),
            types: TypeTable::default(),
            functions: FunctionTable::default(),
            code: Vec::new(),
            local_layouts: LocalLayoutTable::default(),
        }
    }

    /// Encodes the module to a PHX0 byte vector.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError::SectionTooLarge`] when any section or offset exceeds `u32::MAX`.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        let constants = self.constants.encode();
        let types = self.types.encode();
        let functions = self.functions.encode();
        let code = &self.code;
        let local_layouts = self.local_layouts.encode();

        let section_count = 5u32;
        let table_size = 5usize * 12;
        let mut offset = HEADER_SIZE + table_size;

        let constants_entry = SectionEntry {
            kind: SectionKind::Constants,
            offset: u32_len("constants_offset", offset)?,
            length: u32_len("constants", constants.len())?,
        };
        offset = offset.saturating_add(constants.len());

        let types_entry = SectionEntry {
            kind: SectionKind::Types,
            offset: u32_len("types_offset", offset)?,
            length: u32_len("types", types.len())?,
        };
        offset = offset.saturating_add(types.len());

        let functions_entry = SectionEntry {
            kind: SectionKind::Functions,
            offset: u32_len("functions_offset", offset)?,
            length: u32_len("functions", functions.len())?,
        };
        offset = offset.saturating_add(functions.len());

        let code_entry = SectionEntry {
            kind: SectionKind::Code,
            offset: u32_len("code_offset", offset)?,
            length: u32_len("code", code.len())?,
        };
        offset = offset.saturating_add(code.len());

        let local_layouts_entry = SectionEntry {
            kind: SectionKind::LocalLayouts,
            offset: u32_len("local_layouts_offset", offset)?,
            length: u32_len("local_layouts", local_layouts.len())?,
        };

        let header = FileHeader {
            section_count,
            entry_function_id: self.header.entry_function_id,
            ..self.header
        };

        let mut out = Vec::with_capacity(
            offset
                .saturating_add(local_layouts.len())
                .saturating_add(code.len()),
        );
        out.extend_from_slice(&header.encode());
        for entry in [
            constants_entry,
            types_entry,
            functions_entry,
            code_entry,
            local_layouts_entry,
        ] {
            out.extend_from_slice(&entry.encode());
        }
        out.extend_from_slice(&constants);
        out.extend_from_slice(&types);
        out.extend_from_slice(&functions);
        out.extend_from_slice(code);
        out.extend_from_slice(&local_layouts);
        Ok(out)
    }

    /// Decodes a PHX0 file from bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ModuleError`] when the file is truncated or sections are invalid.
    pub fn decode(bytes: &[u8]) -> Result<Self, ModuleError> {
        if bytes.len() < HEADER_SIZE {
            return Err(ModuleError::Truncated);
        }
        let header_bytes: &[u8; HEADER_SIZE] = bytes[0..HEADER_SIZE]
            .try_into()
            .map_err(|_| ModuleError::Truncated)?;
        let header = FileHeader::decode(header_bytes).map_err(ModuleError::Header)?;
        let table_end = HEADER_SIZE.saturating_add(
            usize::try_from(header.section_count)
                .unwrap_or(0)
                .saturating_mul(12),
        );
        if bytes.len() < table_end {
            return Err(ModuleError::Truncated);
        }
        let mut constants = ConstPool::default();
        let mut types = TypeTable::default();
        let mut functions = FunctionTable::default();
        let mut code = Vec::new();
        let mut local_layouts = LocalLayoutTable::default();
        for i in 0..header.section_count {
            let start = HEADER_SIZE + usize::try_from(i).unwrap_or(0).saturating_mul(12);
            let entry_bytes: &[u8; 12] = bytes[start..start + 12]
                .try_into()
                .map_err(|_| ModuleError::Truncated)?;
            let entry = SectionEntry::decode(entry_bytes).map_err(ModuleError::Section)?;
            let off = usize::try_from(entry.offset).unwrap_or(0);
            let len = usize::try_from(entry.length).unwrap_or(0);
            let end = off.saturating_add(len);
            if end > bytes.len() {
                return Err(ModuleError::SectionOutOfBounds);
            }
            let payload = &bytes[off..end];
            match entry.kind {
                SectionKind::Constants => {
                    constants = ConstPool::decode(payload).map_err(ModuleError::Constants)?;
                }
                SectionKind::Types => {
                    types = TypeTable::decode(payload).map_err(ModuleError::Types)?;
                }
                SectionKind::Functions => {
                    functions = FunctionTable::decode(payload).map_err(ModuleError::Functions)?;
                }
                SectionKind::Code => code = payload.to_vec(),
                SectionKind::LocalLayouts => {
                    local_layouts =
                        LocalLayoutTable::decode(payload).map_err(ModuleError::LocalLayouts)?;
                }
                SectionKind::Symbols => {}
            }
        }
        Ok(Self {
            header,
            constants,
            types,
            functions,
            code,
            local_layouts,
        })
    }

    /// Decodes all instructions in the code section.
    ///
    /// # Errors
    ///
    /// Returns [`ModuleError::Instruction`] when bytecode is malformed.
    pub fn decode_instructions(&self) -> Result<Vec<Instruction>, super::instr::InstrError> {
        let mut out = Vec::new();
        let mut offset = 0;
        while offset < self.code.len() {
            let (inst, next) = Instruction::decode_at(&self.code, offset)?;
            out.push(inst);
            offset = next;
        }
        Ok(out)
    }
}

/// Module-level encode/decode failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleError {
    /// Encode failed before write (internal).
    Encode(EncodeError),
    /// File shorter than header or section table.
    Truncated,
    /// Section payload extends past file end.
    SectionOutOfBounds,
    /// Header invalid.
    Header(HeaderError),
    /// Section table entry invalid.
    Section(SectionError),
    /// Constants section invalid.
    Constants(super::const_pool::ConstPoolError),
    /// Types section invalid.
    Types(super::types::TypeTableError),
    /// Functions section invalid.
    Functions(super::function::FunctionTableError),
    /// Local layouts section invalid.
    LocalLayouts(LocalLayoutError),
    /// Instruction stream invalid.
    Instruction(super::instr::InstrError),
}

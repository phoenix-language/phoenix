//! PHX0 [`BytecodeModule`] aggregate and file encode/decode.
//!
//! A module is the in-memory view of a PHX0 file: header, section table, and decoded section
//! payloads. The code section holds a contiguous byte stream of [`Instruction`] records; function
//! metadata in section 3 (`functions`) indexes slices of that stream via `code_offset` /
//! `code_len`.
//!
//! ## PHX0 layout (MVP)
//!
//! | Section | Kind | Contents |
//! |---------|------|----------|
//! | 0 | Constants | [`ConstPool`] entries |
//! | 1 | Types | [`TypeTable`] records |
//! | 2 | Functions | [`FunctionTable`] metadata |
//! | 3 | Code | raw instruction bytes |
//! | 4 | Local layouts | per-function slot kinds |
//! | 5 (optional) | Symbols | [`PcSpanTable`] when debug spans are present |
//!
//! [`BytecodeModule::encode`] and [`BytecodeModule::decode`] are inverse operations for valid
//! modules. Decoding does not run the verifier — call [`super::verify`] before handing a module to
//! the VM.

use super::const_pool::ConstPool;
use super::encode::{EncodeError, u32_len};
use super::function::FunctionTable;
use super::header::{FileHeader, HEADER_SIZE, HeaderError};
use super::instr::Instruction;
use super::local_layout::{LocalLayoutError, LocalLayoutTable};
use super::pc_span::{PHX0_HAS_DEBUG, PcSpanError, PcSpanTable};
use super::section::{SectionEntry, SectionError, SectionKind, validate_section_table};
use super::types::TypeTable;

/// Decoded Phoenix bytecode module (MVP).
///
/// Owns all section payloads for one compilation unit. Use [`BytecodeModule::empty`] for tests
/// and [`BytecodeModule::decode`] to load from disk; [`BytecodeModule::encode`] serializes back
/// to PHX0 bytes.
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
    /// Optional `(function_id, pc) → span` map (section 5 when non-empty).
    pub pc_spans: PcSpanTable,
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
            pc_spans: PcSpanTable::default(),
        }
    }

    /// Returns `true` when section 5 (PC span map) should be written.
    pub(crate) fn writes_pc_spans(&self) -> bool {
        !self.pc_spans.is_empty()
    }

    /// Computes the canonical MVP section table and total file size from in-memory payloads.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError::SectionTooLarge`] when any section or offset exceeds `u32::MAX`.
    pub(crate) fn section_layout(&self) -> Result<(Vec<SectionEntry>, usize), EncodeError> {
        let constants = self.constants.encode();
        let types = self.types.encode();
        let functions = self.functions.encode();
        let code_len = self.code.len();
        let local_layouts = self.local_layouts.encode();

        let base_sections = 5usize;
        let table_size = base_sections.saturating_add(usize::from(self.writes_pc_spans()));
        let table_size = table_size.saturating_mul(12);
        let mut offset = HEADER_SIZE.saturating_add(table_size);

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
            length: u32_len("code", code_len)?,
        };
        offset = offset.saturating_add(code_len);

        let local_layouts_entry = SectionEntry {
            kind: SectionKind::LocalLayouts,
            offset: u32_len("local_layouts_offset", offset)?,
            length: u32_len("local_layouts", local_layouts.len())?,
        };
        offset = offset.saturating_add(local_layouts.len());

        let mut entries = vec![
            constants_entry,
            types_entry,
            functions_entry,
            code_entry,
            local_layouts_entry,
        ];

        if self.writes_pc_spans() {
            let symbols = self.pc_spans.encode();
            entries.push(SectionEntry {
                kind: SectionKind::Symbols,
                offset: u32_len("symbols_offset", offset)?,
                length: u32_len("symbols", symbols.len())?,
            });
            offset = offset.saturating_add(symbols.len());
        }

        Ok((entries, offset))
    }

    /// Encodes the module to a PHX0 byte vector.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError::SectionTooLarge`] when any section or offset exceeds `u32::MAX`.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        let (entries, file_len) = self.section_layout()?;
        let constants = self.constants.encode();
        let types = self.types.encode();
        let functions = self.functions.encode();
        let local_layouts = self.local_layouts.encode();
        let symbols = self.pc_spans.encode();

        let section_count =
            u32::try_from(entries.len()).map_err(|_| EncodeError::SectionTooLarge {
                section: "section_count",
                len: entries.len(),
            })?;

        let mut flags = self.header.flags;
        if self.writes_pc_spans() {
            flags |= PHX0_HAS_DEBUG;
        }

        let header = FileHeader {
            section_count,
            entry_function_id: self.header.entry_function_id,
            flags,
            ..self.header
        };

        let mut out = Vec::with_capacity(file_len);
        out.extend_from_slice(&header.encode());
        for entry in entries {
            out.extend_from_slice(&entry.encode());
        }
        out.extend_from_slice(&constants);
        out.extend_from_slice(&types);
        out.extend_from_slice(&functions);
        out.extend_from_slice(&self.code);
        out.extend_from_slice(&local_layouts);
        if self.writes_pc_spans() {
            out.extend_from_slice(&symbols);
        }
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
        let mut section_entries =
            Vec::with_capacity(usize::try_from(header.section_count).unwrap_or(0));
        for i in 0..header.section_count {
            let start = HEADER_SIZE + usize::try_from(i).unwrap_or(0).saturating_mul(12);
            let entry_bytes: &[u8; 12] = bytes[start..start + 12]
                .try_into()
                .map_err(|_| ModuleError::Truncated)?;
            section_entries.push(SectionEntry::decode(entry_bytes).map_err(ModuleError::Section)?);
        }
        validate_section_table(&section_entries, bytes.len()).map_err(|err| match err {
            SectionError::OutOfBounds => ModuleError::SectionOutOfBounds,
            other => ModuleError::Section(other),
        })?;

        let mut constants = ConstPool::default();
        let mut types = TypeTable::default();
        let mut functions = FunctionTable::default();
        let mut code = Vec::new();
        let mut local_layouts = LocalLayoutTable::default();
        let mut pc_spans = PcSpanTable::default();
        for entry in section_entries {
            let off = usize::try_from(entry.offset).unwrap_or(0);
            let len = usize::try_from(entry.length).unwrap_or(0);
            let end = off.saturating_add(len);
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
                SectionKind::Symbols => {
                    pc_spans = PcSpanTable::decode(payload).map_err(ModuleError::PcSpans)?;
                }
            }
        }
        Ok(Self {
            header,
            constants,
            types,
            functions,
            code,
            local_layouts,
            pc_spans,
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
    /// PC span section invalid.
    PcSpans(PcSpanError),
    /// Instruction stream invalid.
    Instruction(super::instr::InstrError),
}

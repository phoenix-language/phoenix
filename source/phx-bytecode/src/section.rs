//! Section table entries for PHX0 modules.

use std::collections::HashSet;

/// Section kind tags (MVP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum SectionKind {
    /// Constants pool.
    Constants = 1,
    /// Type records.
    Types = 2,
    /// Function metadata.
    Functions = 3,
    /// Instruction bytecode.
    Code = 4,
    /// Debug symbols (optional).
    Symbols = 5,
    /// Per-function local slot layout metadata.
    LocalLayouts = 6,
}

impl SectionKind {
    /// Decodes a section kind from its wire tag.
    ///
    /// # Errors
    ///
    /// Returns [`SectionError::UnknownKind`] for unrecognized tags.
    pub fn from_u16(tag: u16) -> Result<Self, SectionError> {
        match tag {
            1 => Ok(Self::Constants),
            2 => Ok(Self::Types),
            3 => Ok(Self::Functions),
            4 => Ok(Self::Code),
            5 => Ok(Self::Symbols),
            6 => Ok(Self::LocalLayouts),
            _ => Err(SectionError::UnknownKind(tag)),
        }
    }

    /// Returns the wire tag.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self as u16
    }
}

/// One row in the section table (12 bytes on disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionEntry {
    /// Payload kind.
    pub kind: SectionKind,
    /// Byte offset from file start to payload.
    pub offset: u32,
    /// Payload length in bytes.
    pub length: u32,
}

impl SectionEntry {
    /// Encodes one section table entry (12 bytes).
    #[must_use]
    pub fn encode(&self) -> [u8; 12] {
        let mut out = [0u8; 12];
        out[0..2].copy_from_slice(&self.kind.as_u16().to_le_bytes());
        out[4..8].copy_from_slice(&self.offset.to_le_bytes());
        out[8..12].copy_from_slice(&self.length.to_le_bytes());
        out
    }

    /// Decodes one section table entry.
    ///
    /// # Errors
    ///
    /// Returns [`SectionError`] when the kind tag is unknown.
    pub fn decode(bytes: &[u8; 12]) -> Result<Self, SectionError> {
        let tag = u16::from_le_bytes([bytes[0], bytes[1]]);
        Ok(Self {
            kind: SectionKind::from_u16(tag)?,
            offset: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            length: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
        })
    }
}

/// Section table decode errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionError {
    /// Unrecognized `section_kind` tag.
    UnknownKind(u16),
    /// Section payload extends past file end.
    OutOfBounds,
    /// Two section table rows share the same `section_kind`.
    DuplicateKind(SectionKind),
    /// Two section payloads overlap in byte range.
    OverlappingSections {
        /// First overlapping section kind.
        first: SectionKind,
        /// Second overlapping section kind.
        second: SectionKind,
    },
}

/// Validates section table layout: bounds, unique kinds, and non-overlapping ranges.
///
/// # Errors
///
/// Returns [`SectionError`] when any entry is out of bounds, duplicated, or overlaps another.
pub fn validate_section_table(
    entries: &[SectionEntry],
    file_len: usize,
) -> Result<(), SectionError> {
    let file_len_u64 = u64::try_from(file_len).unwrap_or(u64::MAX);
    let mut seen_kinds = HashSet::new();
    for entry in entries {
        let end = u64::from(entry.offset).saturating_add(u64::from(entry.length));
        if end > file_len_u64 {
            return Err(SectionError::OutOfBounds);
        }
        if !seen_kinds.insert(entry.kind) {
            return Err(SectionError::DuplicateKind(entry.kind));
        }
    }
    for (i, left) in entries.iter().enumerate() {
        let left_start = u64::from(left.offset);
        let left_end = left_start.saturating_add(u64::from(left.length));
        for right in entries.iter().skip(i + 1) {
            let right_start = u64::from(right.offset);
            let right_end = right_start.saturating_add(u64::from(right.length));
            if ranges_overlap(left_start, left_end, right_start, right_end) {
                return Err(SectionError::OverlappingSections {
                    first: left.kind,
                    second: right.kind,
                });
            }
        }
    }
    Ok(())
}

fn ranges_overlap(a_start: u64, a_end: u64, b_start: u64, b_end: u64) -> bool {
    a_start < b_end && b_start < a_end
}

//! PHX0 section 5 phase-1 payload: `(function_id, pc) → source span`.
//!
//! Wire format (sub-version 1): see `docs/design/features/debug.md` § "Section 5 phase 1".

use crate::decode::checked_entry_count;

/// Sub-version of the section 5 PC span map (phase 1).
pub const PC_SPAN_SUB_VERSION: u32 = 1;

/// Header flag: section 5 (debug metadata) is present.
pub const PHX0_HAS_DEBUG: u32 = 0x0000_0001;

/// One `(function_id, pc)` site mapped to a source file and byte span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcSpanEntry {
    /// Owning function id (PC is relative to that function's code body).
    pub function_id: u32,
    /// Byte offset of the instruction within the function body (same as VM `VmError` pc).
    pub pc: u32,
    /// Index into [`PcSpanTable::files`].
    pub file_id: u32,
    /// Inclusive start byte offset in the UTF-8 source file.
    pub span_start: u32,
    /// Exclusive end byte offset in the UTF-8 source file.
    pub span_end: u32,
}

impl PcSpanEntry {
    /// Creates one mapping row.
    #[must_use]
    pub const fn new(
        function_id: u32,
        pc: u32,
        file_id: u32,
        span_start: u32,
        span_end: u32,
    ) -> Self {
        Self {
            function_id,
            pc,
            file_id,
            span_start,
            span_end,
        }
    }
}

/// PC span map and compilation-unit paths for section 5 (phase 1).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PcSpanTable {
    /// Project-relative or logical source paths (`file_id` indexes this vec).
    pub files: Vec<String>,
    /// Rows sorted by `(function_id, pc)` ascending.
    pub entries: Vec<PcSpanEntry>,
}

impl PcSpanTable {
    /// Returns `true` when the table has no serialized payload beyond empty headers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.entries.is_empty()
    }

    /// Inserts a row and keeps [`Self::entries`] sorted.
    pub fn push_sorted(&mut self, entry: PcSpanEntry) {
        match self
            .entries
            .binary_search_by_key(&(entry.function_id, entry.pc), |e| (e.function_id, e.pc))
        {
            Ok(idx) => self.entries[idx] = entry,
            Err(idx) => self.entries.insert(idx, entry),
        }
    }

    /// Returns the span entry for an exact `(function_id, pc)` site, if recorded.
    #[must_use]
    pub fn lookup_exact(&self, function_id: u32, pc: u32) -> Option<&PcSpanEntry> {
        self.entries
            .binary_search_by_key(&(function_id, pc), |e| (e.function_id, e.pc))
            .ok()
            .map(|idx| &self.entries[idx])
    }

    /// Returns the entry with the greatest `pc` not exceeding `pc` for `function_id`.
    #[must_use]
    pub fn lookup_at_or_before(&self, function_id: u32, pc: u32) -> Option<&PcSpanEntry> {
        let start = self
            .entries
            .partition_point(|e| e.function_id < function_id);
        let end = self
            .entries
            .partition_point(|e| e.function_id <= function_id);
        let slice = &self.entries[start..end];
        let idx = slice.partition_point(|e| e.pc <= pc);
        if idx == 0 {
            None
        } else {
            Some(&slice[idx - 1])
        }
    }

    /// Merges `other` into `self`, rebasing `file_id` values in `other`'s rows.
    pub fn merge_from(&mut self, other: PcSpanTable) {
        let mut file_remap = Vec::with_capacity(other.files.len());
        for path in other.files {
            let id = if let Some(pos) = self.files.iter().position(|p| p == &path) {
                u32::try_from(pos).unwrap_or(u32::MAX)
            } else {
                let id = u32::try_from(self.files.len()).unwrap_or(u32::MAX);
                self.files.push(path);
                id
            };
            file_remap.push(id);
        }
        for mut entry in other.entries {
            if let Some(remapped) = file_remap.get(entry.file_id as usize) {
                entry.file_id = *remapped;
            }
            self.push_sorted(entry);
        }
    }

    /// Encodes the section 5 phase-1 payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&PC_SPAN_SUB_VERSION.to_le_bytes());

        let file_count = u32::try_from(self.files.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&file_count.to_le_bytes());
        for path in &self.files {
            let bytes = path.as_bytes();
            let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(bytes);
        }

        let entry_count = u32::try_from(self.entries.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&entry_count.to_le_bytes());
        for entry in &self.entries {
            out.extend_from_slice(&entry.function_id.to_le_bytes());
            out.extend_from_slice(&entry.pc.to_le_bytes());
            out.extend_from_slice(&entry.file_id.to_le_bytes());
            out.extend_from_slice(&entry.span_start.to_le_bytes());
            out.extend_from_slice(&entry.span_end.to_le_bytes());
        }
        out
    }

    /// Decodes a section 5 phase-1 payload.
    ///
    /// # Errors
    ///
    /// Returns [`PcSpanError`] when the payload is truncated or malformed.
    pub fn decode(payload: &[u8]) -> Result<Self, PcSpanError> {
        if payload.len() < 8 {
            return Err(PcSpanError::Truncated);
        }
        let sub_version = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        if sub_version != PC_SPAN_SUB_VERSION {
            return Err(PcSpanError::UnsupportedSubVersion { found: sub_version });
        }
        let mut offset = 4usize;
        let file_count = read_u32(payload, &mut offset)?;
        let mut files = Vec::with_capacity(usize::try_from(file_count).unwrap_or(0));
        for _ in 0..file_count {
            let len = usize::try_from(read_u32(payload, &mut offset)?)
                .map_err(|_| PcSpanError::InvalidStringLen { len: u32::MAX })?;
            let end = offset.saturating_add(len);
            if end > payload.len() {
                return Err(PcSpanError::Truncated);
            }
            let path = std::str::from_utf8(&payload[offset..end])
                .map_err(|_| PcSpanError::InvalidUtf8Path)?
                .to_owned();
            files.push(path);
            offset = end;
        }

        let entry_count = read_u32(payload, &mut offset)?;
        let entry_count_usize = usize::try_from(entry_count).map_err(|_| PcSpanError::Truncated)?;
        let entries =
            checked_entry_count(payload.len().saturating_sub(offset), 20, entry_count_usize)
                .ok_or(PcSpanError::Truncated)?;
        let mut rows = Vec::with_capacity(entries);
        for _ in 0..entries {
            if offset.saturating_add(20) > payload.len() {
                return Err(PcSpanError::Truncated);
            }
            let function_id = read_u32(payload, &mut offset)?;
            let pc = read_u32(payload, &mut offset)?;
            let file_id = read_u32(payload, &mut offset)?;
            let span_start = read_u32(payload, &mut offset)?;
            let span_end = read_u32(payload, &mut offset)?;
            if span_end < span_start {
                return Err(PcSpanError::InvalidSpan {
                    span_start,
                    span_end,
                });
            }
            if file_count == 0 {
                if file_id != 0 {
                    return Err(PcSpanError::FileIdOutOfRange {
                        file_id,
                        file_count,
                    });
                }
            } else if file_id >= file_count {
                return Err(PcSpanError::FileIdOutOfRange {
                    file_id,
                    file_count,
                });
            }
            rows.push(PcSpanEntry::new(
                function_id,
                pc,
                file_id,
                span_start,
                span_end,
            ));
        }
        if !rows
            .windows(2)
            .all(|w| (w[0].function_id, w[0].pc) <= (w[1].function_id, w[1].pc))
        {
            return Err(PcSpanError::UnsortedEntries);
        }
        Ok(Self {
            files,
            entries: rows,
        })
    }
}

fn read_u32(payload: &[u8], offset: &mut usize) -> Result<u32, PcSpanError> {
    let end = offset.saturating_add(4);
    if end > payload.len() {
        return Err(PcSpanError::Truncated);
    }
    let value = u32::from_le_bytes([
        payload[*offset],
        payload[*offset + 1],
        payload[*offset + 2],
        payload[*offset + 3],
    ]);
    *offset = end;
    Ok(value)
}

/// Decode failures for [`PcSpanTable`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PcSpanError {
    /// Payload shorter than required.
    Truncated,
    /// Unknown sub-version.
    UnsupportedSubVersion {
        /// Version read from file.
        found: u32,
    },
    /// Path bytes are not UTF-8.
    InvalidUtf8Path,
    /// Declared path length overflows `usize`.
    InvalidStringLen {
        /// Length from file.
        len: u32,
    },
    /// `span_end` precedes `span_start`.
    InvalidSpan {
        /// Start offset.
        span_start: u32,
        /// End offset.
        span_end: u32,
    },
    /// Entry references a missing file row.
    FileIdOutOfRange {
        /// Referenced id.
        file_id: u32,
        /// Declared file count.
        file_count: u32,
    },
    /// Rows are not sorted by `(function_id, pc)`.
    UnsortedEntries,
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn pc_span_table_round_trip() {
        let mut table = PcSpanTable {
            files: vec!["src/main.phx".to_owned()],
            entries: Vec::new(),
        };
        table.push_sorted(PcSpanEntry::new(0, 0, 0, 10, 14));
        table.push_sorted(PcSpanEntry::new(0, 12, 0, 20, 25));
        table.push_sorted(PcSpanEntry::new(1, 0, 0, 100, 110));

        let bytes = table.encode();
        let decoded = PcSpanTable::decode(&bytes).expect("decode");
        assert_eq!(decoded, table);
    }

    #[test]
    fn lookup_at_or_before_picks_nearest_pc() {
        let table = PcSpanTable {
            files: vec!["a.phx".to_owned()],
            entries: vec![
                PcSpanEntry::new(2, 0, 0, 1, 2),
                PcSpanEntry::new(2, 8, 0, 3, 4),
                PcSpanEntry::new(2, 16, 0, 5, 6),
            ],
        };
        assert_eq!(
            table.lookup_at_or_before(2, 10).map(|e| e.span_start),
            Some(3)
        );
        assert_eq!(
            table.lookup_at_or_before(2, 8).map(|e| e.span_start),
            Some(3)
        );
        assert!(table.lookup_at_or_before(3, 0).is_none());
    }

    #[test]
    fn merge_rebases_file_ids() {
        let mut left = PcSpanTable {
            files: vec!["shared.phx".to_owned()],
            entries: vec![PcSpanEntry::new(0, 0, 0, 1, 2)],
        };
        let right = PcSpanTable {
            files: vec!["shared.phx".to_owned(), "other.phx".to_owned()],
            entries: vec![PcSpanEntry::new(1, 4, 1, 9, 10)],
        };
        left.merge_from(right);
        assert_eq!(left.files, vec!["shared.phx", "other.phx"]);
        assert_eq!(left.entries[1].file_id, 1);
    }
}

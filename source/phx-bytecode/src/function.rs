//! Function metadata records.

use crate::decode::checked_entry_count;

/// One function entry in the functions section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionRecord {
    /// Unique function id.
    pub function_id: u32,
    /// Symbol table id for name (0 if none).
    pub name_symbol_id: u32,
    /// Parameter count.
    pub arity: u16,
    /// Local slot count (params + locals).
    pub local_count: u16,
    /// Maximum operand stack depth (verifier).
    pub stack_max: u16,
    /// Function flags (reserved MVP = 0).
    pub flags: u16,
    /// Offset into code section.
    pub code_offset: u32,
    /// Byte length of function body in code section.
    pub code_len: u32,
    /// Return type id in type table.
    pub return_type_id: u32,
}

/// Functions section body.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FunctionTable {
    /// All functions.
    pub functions: Vec<FunctionRecord>,
}

impl FunctionTable {
    /// Encodes the functions section payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let count = u32::try_from(self.functions.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&count.to_le_bytes());
        for f in &self.functions {
            out.extend_from_slice(&f.function_id.to_le_bytes());
            out.extend_from_slice(&f.name_symbol_id.to_le_bytes());
            out.extend_from_slice(&f.arity.to_le_bytes());
            out.extend_from_slice(&f.local_count.to_le_bytes());
            out.extend_from_slice(&f.stack_max.to_le_bytes());
            out.extend_from_slice(&f.flags.to_le_bytes());
            out.extend_from_slice(&f.code_offset.to_le_bytes());
            out.extend_from_slice(&f.code_len.to_le_bytes());
            out.extend_from_slice(&f.return_type_id.to_le_bytes());
        }
        out
    }

    /// Decodes a functions section payload.
    ///
    /// # Errors
    ///
    /// Returns `FunctionTableError::Truncated` when bytes are incomplete.
    pub fn decode(bytes: &[u8]) -> Result<Self, FunctionTableError> {
        const RECORD_SIZE: usize = 28;
        if bytes.len() < 4 {
            return Err(FunctionTableError::Truncated);
        }
        let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let remaining = bytes.len().saturating_sub(4);
        let count = checked_entry_count(remaining, RECORD_SIZE, count)
            .ok_or(FunctionTableError::Truncated)?;
        let mut functions = Vec::with_capacity(count);
        let mut pos = 4;
        for _ in 0..count {
            let function_id = read_u32(bytes, pos);
            let name_symbol_id = read_u32(bytes, pos + 4);
            let arity = u16::from_le_bytes([bytes[pos + 8], bytes[pos + 9]]);
            let local_count = u16::from_le_bytes([bytes[pos + 10], bytes[pos + 11]]);
            let stack_max = u16::from_le_bytes([bytes[pos + 12], bytes[pos + 13]]);
            let flags = u16::from_le_bytes([bytes[pos + 14], bytes[pos + 15]]);
            let code_offset = read_u32(bytes, pos + 16);
            let code_len = read_u32(bytes, pos + 20);
            let return_type_id = read_u32(bytes, pos + 24);
            functions.push(FunctionRecord {
                function_id,
                name_symbol_id,
                arity,
                local_count,
                stack_max,
                flags,
                code_offset,
                code_len,
                return_type_id,
            });
            pos += RECORD_SIZE;
        }
        Ok(Self { functions })
    }
}

fn read_u32(bytes: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
}

/// Function table decode errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionTableError {
    /// Unexpected end of payload.
    Truncated,
}

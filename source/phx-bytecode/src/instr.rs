//! Variable-length logical instructions (`opcode` + `operand_count` + operands).

use super::opcode::Opcode;

/// One decoded instruction (operands are `u32` indices only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Opcode discriminant.
    pub opcode: Opcode,
    /// Operand indices (locals, constants, jump targets, etc.).
    pub operands: Vec<u32>,
}

impl Instruction {
    /// Encodes to wire form: `u8 opcode`, `u8 operand_count`, `operand_count × u32`.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let count = u8::try_from(self.operands.len()).unwrap_or(u8::MAX);
        let mut out = Vec::with_capacity(2 + self.operands.len() * 4);
        out.push(self.opcode.as_u8());
        out.push(count);
        for op in self.operands.iter().take(usize::from(count)) {
            out.extend_from_slice(&op.to_le_bytes());
        }
        out
    }

    /// Decodes one instruction from `bytes` starting at `offset`; returns (instruction, new offset).
    ///
    /// # Errors
    ///
    /// Returns [`InstrError`] when the slice is truncated or opcode is invalid.
    pub fn decode_at(bytes: &[u8], offset: usize) -> Result<(Self, usize), InstrError> {
        let opcode_byte = *bytes.get(offset).ok_or(InstrError::Truncated)?;
        let opcode = Opcode::from_u8(opcode_byte).map_err(InstrError::Opcode)?;
        let count_offset = offset + 1;
        let count = usize::from(*bytes.get(count_offset).ok_or(InstrError::Truncated)?);
        let mut operands = Vec::with_capacity(count);
        let mut pos = count_offset + 1;
        for _ in 0..count {
            let end = pos.saturating_add(4);
            if end > bytes.len() {
                return Err(InstrError::Truncated);
            }
            let word =
                u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
            operands.push(word);
            pos = end;
        }
        Ok((Self { opcode, operands }, pos))
    }
}

/// Instruction decode errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstrError {
    /// Unexpected end of bytecode.
    Truncated,
    /// Invalid opcode byte.
    Opcode(super::opcode::OpcodeError),
}

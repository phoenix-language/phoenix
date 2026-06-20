//! Variable-length logical instructions (`opcode` + `operand_count` + operands).
//!
//! Each [`Instruction`] is the decoded view of one record in the code section. Operands are always
//! `u32` indices — constant pool slots, type table rows, local slots, jump targets, or
//! callee ids depending on the [`Opcode`]. Stack effects for each opcode are documented on
//! [`Opcode`].
//!
//! ## Wire format
//!
//! ```text
//! u8 opcode | u8 operand_count | operand_count × u32 (little-endian)
//! ```
//!
//! [`Instruction::encode`] and [`Instruction::decode_at`] round-trip this layout. Operand count is
//! capped at 255 ([`InstrError::TooManyOperands`]). [`Instruction::apply_link_bases`] rebases
//! constant-pool and type-table indices when merging object files; function ids and jump targets
//! are left unchanged.

use super::decode::checked_entry_count;
use super::opcode::Opcode;

/// One decoded instruction (operands are `u32` indices only).
///
/// Construct via [`Instruction::decode_at`] or by filling `opcode` and `operands` directly before
/// [`Instruction::encode`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Opcode discriminant.
    pub opcode: Opcode,
    /// Operand indices (locals, constants, jump targets, etc.).
    pub operands: Vec<u32>,
}

impl Instruction {
    /// Encodes to wire form: `u8 opcode`, `u8 operand_count`, `operand_count × u32`.
    ///
    /// # Errors
    ///
    /// Returns [`InstrError::TooManyOperands`] when `operands.len()` exceeds 255.
    pub fn encode(&self) -> Result<Vec<u8>, InstrError> {
        let count = u8::try_from(self.operands.len()).map_err(|_| InstrError::TooManyOperands {
            count: self.operands.len(),
            max: u8::MAX,
        })?;
        let mut out = Vec::with_capacity(2 + self.operands.len() * 4);
        out.push(self.opcode.as_u8());
        out.push(count);
        for op in &self.operands {
            out.extend_from_slice(&op.to_le_bytes());
        }
        Ok(out)
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
        let remaining = bytes.len().saturating_sub(count_offset + 1);
        let count = checked_entry_count(remaining, 4, count).ok_or(InstrError::Truncated)?;
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

    /// Rebases constant-pool and type-table operand indices when merging module object files.
    ///
    /// [`Opcode::Call`] and [`Opcode::MakeFnPtr`] operands are globally assigned function ids
    /// and are intentionally unchanged. Jump targets, local slots, and primitive-kind operands
    /// are function-local or wire bytes and are also unchanged.
    #[must_use]
    pub fn apply_link_bases(&self, const_base: u32, type_base: u32) -> Self {
        let mut ops = self.operands.clone();
        match self.opcode {
            Opcode::Const | Opcode::MakeStr => {
                if let Some(slot) = ops.first_mut() {
                    *slot = slot.saturating_add(const_base);
                }
            }
            Opcode::MakeStruct
            | Opcode::MakeEnum
            | Opcode::GetField
            | Opcode::SetField
            | Opcode::MatchTag => {
                if let Some(slot) = ops.first_mut() {
                    *slot = slot.saturating_add(type_base);
                }
            }
            Opcode::CallIndirect => {
                if let Some(slot) = ops.get_mut(1) {
                    *slot = slot.saturating_add(type_base);
                }
            }
            Opcode::LoadLocal
            | Opcode::StoreLocal
            | Opcode::Pop
            | Opcode::Add
            | Opcode::Sub
            | Opcode::Mul
            | Opcode::Div
            | Opcode::Eq
            | Opcode::Lt
            | Opcode::Jump
            | Opcode::JumpIfTrue
            | Opcode::JumpIfFalse
            | Opcode::Return
            | Opcode::Call
            | Opcode::Cast
            | Opcode::Mod
            | Opcode::Pow
            | Opcode::Neg
            | Opcode::Not
            | Opcode::BitNot
            | Opcode::BitAnd
            | Opcode::BitOr
            | Opcode::BitXor
            | Opcode::Shl
            | Opcode::Shr
            | Opcode::Ne
            | Opcode::Le
            | Opcode::Ge
            | Opcode::MakeTuple
            | Opcode::MakeArray
            | Opcode::Index
            | Opcode::Trap
            | Opcode::Alloc
            | Opcode::PtrLoad
            | Opcode::PtrStore
            | Opcode::MakeSlice
            | Opcode::AddressOfLocal
            | Opcode::StrAsSlice
            | Opcode::SliceLen
            | Opcode::MakeFnPtr
            | Opcode::LoadAggViaLocalPtr
            | Opcode::MakeSliceFromPtr
            | Opcode::Free
            | Opcode::IndexStore => {}
        }
        Self {
            opcode: self.opcode,
            operands: ops,
        }
    }
}

/// Instruction encode/decode errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstrError {
    /// Unexpected end of bytecode.
    Truncated,
    /// Invalid opcode byte.
    Opcode(super::opcode::OpcodeError),
    /// Operand count exceeds the wire-format `u8` limit.
    TooManyOperands {
        /// Requested operand count.
        count: usize,
        /// Maximum encodable count (255).
        max: u8,
    },
}

impl std::fmt::Display for InstrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated instruction"),
            Self::Opcode(err) => write!(f, "invalid opcode: {err:?}"),
            Self::TooManyOperands { count, max } => {
                write!(f, "instruction has {count} operands; maximum is {max}")
            }
        }
    }
}

impl std::error::Error for InstrError {}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::Opcode;

    #[test]
    fn instruction_encode_rejects_too_many_operands() {
        let inst = Instruction {
            opcode: Opcode::Const,
            operands: vec![0; 256],
        };
        let err = inst.encode().unwrap_err();
        assert_eq!(
            err,
            InstrError::TooManyOperands {
                count: 256,
                max: u8::MAX,
            }
        );
    }

    #[test]
    fn apply_link_bases_rebases_const_and_type_operands() {
        let cases = [
            (
                Instruction {
                    opcode: Opcode::Const,
                    operands: vec![2, 0],
                },
                vec![7, 0],
            ),
            (
                Instruction {
                    opcode: Opcode::MakeStr,
                    operands: vec![1],
                },
                vec![6],
            ),
            (
                Instruction {
                    opcode: Opcode::GetField,
                    operands: vec![0, 1],
                },
                vec![3, 1],
            ),
            (
                Instruction {
                    opcode: Opcode::MatchTag,
                    operands: vec![0, 2],
                },
                vec![3, 2],
            ),
            (
                Instruction {
                    opcode: Opcode::CallIndirect,
                    operands: vec![1, 4],
                },
                vec![1, 7],
            ),
        ];
        for (inst, expected) in cases {
            let patched = inst.apply_link_bases(5, 3);
            assert_eq!(patched.operands, expected, "{:?}", inst.opcode);
        }
    }

    #[test]
    fn apply_link_bases_leaves_non_pool_operands_unchanged() {
        let cases = [
            Instruction {
                opcode: Opcode::MakeTuple,
                operands: vec![2],
            },
            Instruction {
                opcode: Opcode::MakeArray,
                operands: vec![4],
            },
            Instruction {
                opcode: Opcode::Call,
                operands: vec![9],
            },
            Instruction {
                opcode: Opcode::Cast,
                operands: vec![1, 2],
            },
        ];
        for inst in cases {
            let expected = inst.operands.clone();
            let patched = inst.apply_link_bases(5, 3);
            assert_eq!(patched.operands, expected, "{:?}", inst.opcode);
        }
    }
}

//! PHX0 constants section (kind `1`) — literal pool for `CONST` operands.
//!
//! The compiler lowers Phoenix literals and static byte blobs into a pooled table; the VM loads
//! values via [`ConstEntry`] indices referenced by [`crate::Instruction`] operands. The verifier
//! rejects entries whose payload width does not match the consuming instruction's primitive kind.
//!
//! Wire layout: `docs/design/features/vm-linear.md` § "Constants section".
//!
//! ## Payload layout
//!
//! `u32` entry count, then per entry: `const_tag` (1), reserved (1), `byte_len` (2), payload.
//!
//! ## In this module
//!
//! - [`ConstTag`] — wire discriminant for signed/unsigned/float/bytes/bool payloads.
//! - [`ConstEntry`] — one tagged payload row.
//! - [`ConstPool`] — full section body; [`ConstPool::encode`] / [`ConstPool::decode`].

use crate::decode::checked_entry_count;

/// Constant entry tag (wire `u8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConstTag {
    /// Signed integer (`s64` wire payload per `vm-linear.md`).
    SignedInt = 1,
    /// Unsigned integer (`u64` payload).
    UnsignedInt = 2,
    /// `f32` payload.
    Float32 = 3,
    /// `f64` payload.
    Float64 = 4,
    /// Raw byte blob.
    Bytes = 5,
    /// Boolean (`u8` 0/1).
    Bool = 6,
}

/// One constant pool entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstEntry {
    /// Entry kind.
    pub tag: ConstTag,
    /// Payload bytes (interpretation depends on `tag`).
    pub payload: Vec<u8>,
}

/// Constants section body.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConstPool {
    /// All constants in pool order.
    pub entries: Vec<ConstEntry>,
}

impl ConstPool {
    /// Encodes the section payload (`u32` count + entries).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let count = u32::try_from(self.entries.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&count.to_le_bytes());
        for entry in &self.entries {
            let len = u16::try_from(entry.payload.len()).unwrap_or(u16::MAX);
            out.push(entry.tag as u8);
            out.push(0);
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&entry.payload);
        }
        out
    }

    /// Decodes a constants section payload.
    ///
    /// # Errors
    ///
    /// Returns `ConstPoolError::Truncated` when bytes are incomplete.
    pub fn decode(bytes: &[u8]) -> Result<Self, ConstPoolError> {
        if bytes.len() < 4 {
            return Err(ConstPoolError::Truncated);
        }
        let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let remaining = bytes.len().saturating_sub(4);
        let count = checked_entry_count(remaining, 4, count).ok_or(ConstPoolError::Truncated)?;
        let mut entries = Vec::with_capacity(count);
        let mut pos = 4;
        for _ in 0..count {
            if pos + 4 > bytes.len() {
                return Err(ConstPoolError::Truncated);
            }
            let tag_byte = bytes[pos];
            let len = u16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]]) as usize;
            pos += 4;
            let end = pos.saturating_add(len);
            if end > bytes.len() {
                return Err(ConstPoolError::Truncated);
            }
            let tag = match tag_byte {
                1 => ConstTag::SignedInt,
                2 => ConstTag::UnsignedInt,
                3 => ConstTag::Float32,
                4 => ConstTag::Float64,
                5 => ConstTag::Bytes,
                6 => ConstTag::Bool,
                _ => return Err(ConstPoolError::UnknownTag(tag_byte)),
            };
            entries.push(ConstEntry {
                tag,
                payload: bytes[pos..end].to_vec(),
            });
            pos = end;
        }
        Ok(Self { entries })
    }
}

/// Constants section decode errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstPoolError {
    /// Unexpected end of payload.
    Truncated,
    /// Unknown `const_tag`.
    UnknownTag(u8),
}

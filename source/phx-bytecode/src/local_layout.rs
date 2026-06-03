//! Per-function local slot layout metadata for typed load/store.

use crate::cast::{PrimitiveKind, SLOT_KIND_AGG};

/// One local slot descriptor (`0xFF` = aggregate, else [`PrimitiveKind`] wire byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalSlotKind(pub u8);

impl LocalSlotKind {
    /// Aggregate slot (struct, enum, tuple, array, slice).
    #[must_use]
    pub const fn aggregate() -> Self {
        Self(SLOT_KIND_AGG)
    }

    /// Primitive slot.
    #[must_use]
    pub const fn primitive(kind: PrimitiveKind) -> Self {
        Self(kind.as_u8())
    }

    /// Returns `true` for aggregate slots.
    #[must_use]
    pub const fn is_aggregate(self) -> bool {
        self.0 == SLOT_KIND_AGG
    }

    /// Returns the primitive kind when not aggregate.
    #[must_use]
    pub fn primitive_kind(self) -> Option<PrimitiveKind> {
        if self.is_aggregate() {
            None
        } else {
            PrimitiveKind::from_u8(self.0)
        }
    }
}

/// Local slot kinds for one function, indexed by slot number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionLocalLayout {
    /// Owning function id.
    pub function_id: u32,
    /// Slot kinds in slot order.
    pub slots: Vec<LocalSlotKind>,
}

/// All function local layouts in a module.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalLayoutTable {
    /// Layout rows keyed by function id order in the table.
    pub layouts: Vec<FunctionLocalLayout>,
}

impl LocalLayoutTable {
    /// Encodes the local layouts section payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let count = u32::try_from(self.layouts.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&count.to_le_bytes());
        for layout in &self.layouts {
            out.extend_from_slice(&layout.function_id.to_le_bytes());
            let slot_count = u16::try_from(layout.slots.len()).unwrap_or(u16::MAX);
            out.extend_from_slice(&slot_count.to_le_bytes());
            for slot in &layout.slots {
                out.push(slot.0);
            }
        }
        out
    }

    /// Decodes a local layouts section payload.
    ///
    /// # Errors
    ///
    /// Returns [`LocalLayoutError::Truncated`] when bytes are incomplete.
    pub fn decode(bytes: &[u8]) -> Result<Self, LocalLayoutError> {
        if bytes.len() < 4 {
            return Err(LocalLayoutError::Truncated);
        }
        let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let mut layouts = Vec::with_capacity(count);
        let mut pos = 4;
        for _ in 0..count {
            if pos + 6 > bytes.len() {
                return Err(LocalLayoutError::Truncated);
            }
            let function_id =
                u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
            let slot_count = u16::from_le_bytes([bytes[pos + 4], bytes[pos + 5]]) as usize;
            pos += 6;
            let end = pos.saturating_add(slot_count);
            if end > bytes.len() {
                return Err(LocalLayoutError::Truncated);
            }
            let slots = bytes[pos..end].iter().map(|&b| LocalSlotKind(b)).collect();
            layouts.push(FunctionLocalLayout { function_id, slots });
            pos = end;
        }
        Ok(Self { layouts })
    }

    /// Looks up layout for `function_id`.
    #[must_use]
    pub fn for_function(&self, function_id: u32) -> Option<&FunctionLocalLayout> {
        self.layouts.iter().find(|l| l.function_id == function_id)
    }
}

/// Local layout decode errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalLayoutError {
    /// Unexpected end of payload.
    Truncated,
}

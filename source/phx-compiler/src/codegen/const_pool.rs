//! Module constant pool builder (dedupe literal payloads).

use phx_bytecode::{ConstEntry, ConstPool, ConstTag};

use crate::ir::IrInst;

/// Builder for a deduplicated [`ConstPool`].
#[derive(Debug, Default)]
pub struct ConstPoolBuilder {
    entries: Vec<ConstEntry>,
    /// Maps raw IR const bits → pool index.
    raw_to_pool: std::collections::HashMap<u32, u32>,
}

impl ConstPoolBuilder {
    /// Creates an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns pool index for an IR [`IrInst::Const`] raw `index` field.
    #[must_use]
    pub fn intern_raw(&mut self, raw: u32) -> u32 {
        if let Some(&idx) = self.raw_to_pool.get(&raw) {
            return idx;
        }
        let entry = raw_to_entry(raw);
        let idx = u32::try_from(self.entries.len()).unwrap_or(u32::MAX);
        self.entries.push(entry);
        self.raw_to_pool.insert(raw, idx);
        idx
    }

    /// Collects all [`IrInst::Const`] from `insts`.
    pub fn collect_insts(&mut self, insts: &[IrInst]) {
        for inst in insts {
            if let IrInst::Const { index, .. } = inst {
                let _ = self.intern_raw(*index);
            }
        }
    }

    /// Finishes the pool.
    #[must_use]
    pub fn finish(self) -> ConstPool {
        ConstPool {
            entries: self.entries,
        }
    }
}

fn raw_to_entry(raw: u32) -> ConstEntry {
    let value = i64::from(raw.cast_signed());
    ConstEntry {
        tag: ConstTag::SignedInt,
        payload: value.to_le_bytes().to_vec(),
    }
}

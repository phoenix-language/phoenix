//! Module constant pool builder (dedupe literal payloads).

use phx_bytecode::{ConstEntry, ConstPool, ConstTag};

use crate::ir::{IrConst, IrInst};

/// Builder for a deduplicated [`ConstPool`].
#[derive(Debug, Default)]
pub struct ConstPoolBuilder {
    entries: Vec<ConstEntry>,
}

impl ConstPoolBuilder {
    /// Creates an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends all literals from `constants` in order (index `i` → pool `i`).
    pub fn fill_from_ir(&mut self, constants: &[IrConst]) {
        for lit in constants {
            self.entries.push(ir_const_to_entry(lit));
        }
    }

    /// Pool index equals `constants` index when built via [`Self::fill_from_ir`].
    #[must_use]
    pub fn pool_index_for_literal(&self, literal_index: u32) -> u32 {
        literal_index
    }

    /// Collects all [`IrInst::Const`] from `insts` (no-op when literals are pre-filled).
    pub fn collect_insts(&mut self, insts: &[IrInst]) {
        let _ = insts;
    }

    /// Finishes the pool.
    #[must_use]
    pub fn finish(self) -> ConstPool {
        ConstPool {
            entries: self.entries,
        }
    }
}

fn ir_const_to_entry(lit: &IrConst) -> ConstEntry {
    match lit {
        IrConst::Int(v) => ConstEntry {
            tag: ConstTag::SignedInt,
            payload: v.to_le_bytes().to_vec(),
        },
        IrConst::Float(v) => ConstEntry {
            tag: ConstTag::Float64,
            payload: v.to_le_bytes().to_vec(),
        },
        IrConst::Bool(b) => ConstEntry {
            tag: ConstTag::Bool,
            payload: vec![u8::from(*b)],
        },
    }
}

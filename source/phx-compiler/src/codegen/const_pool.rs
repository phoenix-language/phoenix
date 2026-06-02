//! Module constant pool builder (dedupe literal payloads).

use phx_bytecode::{ConstEntry, ConstPool, ConstTag, PrimitiveKind};

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
        IrConst::Int(v, kind) => {
            let bytes = scalar_bytes_i128(*v, *kind);
            ConstEntry {
                tag: if kind.is_unsigned() {
                    ConstTag::UnsignedInt
                } else {
                    ConstTag::SignedInt
                },
                payload: bytes,
            }
        }
        IrConst::Float(v, kind) => ConstEntry {
            tag: match kind {
                PrimitiveKind::F32 => ConstTag::Float32,
                _ => ConstTag::Float64,
            },
            payload: match kind {
                PrimitiveKind::F32 => (*v as f32).to_le_bytes().to_vec(),
                _ => v.to_le_bytes().to_vec(),
            },
        },
        IrConst::Bool(b) => ConstEntry {
            tag: ConstTag::Bool,
            payload: vec![u8::from(*b)],
        },
        IrConst::Bytes(b) => ConstEntry {
            tag: ConstTag::Bytes,
            payload: b.clone(),
        },
    }
}

fn scalar_bytes_i128(v: i128, kind: PrimitiveKind) -> Vec<u8> {
    match kind {
        PrimitiveKind::S8 => (v as i8).to_le_bytes().to_vec(),
        PrimitiveKind::U8 => (v as u8).to_le_bytes().to_vec(),
        PrimitiveKind::S16 => (v as i16).to_le_bytes().to_vec(),
        PrimitiveKind::U16 => (v as u16).to_le_bytes().to_vec(),
        PrimitiveKind::S32 => (v as i32).to_le_bytes().to_vec(),
        PrimitiveKind::U32 => (v as u32).to_le_bytes().to_vec(),
        PrimitiveKind::S64 => (v as i64).to_le_bytes().to_vec(),
        PrimitiveKind::U64 => (v as u64).to_le_bytes().to_vec(),
        PrimitiveKind::S128 => v.to_le_bytes().to_vec(),
        PrimitiveKind::U128 => (v as u128).to_le_bytes().to_vec(),
        PrimitiveKind::Bool => vec![u8::from(v != 0)],
        PrimitiveKind::F32 => (v as f32).to_le_bytes().to_vec(),
        PrimitiveKind::F64 => (v as f64).to_le_bytes().to_vec(),
    }
}

trait PrimUnsigned {
    fn is_unsigned(self) -> bool;
}

impl PrimUnsigned for PrimitiveKind {
    fn is_unsigned(self) -> bool {
        matches!(
            self,
            PrimitiveKind::U8
                | PrimitiveKind::U16
                | PrimitiveKind::U32
                | PrimitiveKind::U64
                | PrimitiveKind::U128
        )
    }
}

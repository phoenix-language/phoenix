//! Module constant pool builder (dedupe literal payloads).

use std::collections::HashMap;

use phx_bytecode::{ConstEntry, ConstPool, ConstTag, PrimitiveKind};

use crate::ir::IrConst;

/// Key for deduplicating constant pool entries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PoolKey {
    tag: u8,
    payload: Vec<u8>,
}

/// Builder for a deduplicated [`ConstPool`].
#[derive(Debug, Default)]
pub struct ConstPoolBuilder {
    entries: Vec<ConstEntry>,
    dedupe: HashMap<PoolKey, u32>,
    /// Maps IR literal index → pool index.
    ir_to_pool: Vec<u32>,
}

impl ConstPoolBuilder {
    /// Creates an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers all literals from `constants`, deduplicating identical payloads.
    pub fn fill_from_ir(&mut self, constants: &[IrConst]) {
        self.ir_to_pool.clear();
        self.ir_to_pool.reserve(constants.len());
        for lit in constants {
            let entry = ir_const_to_entry(lit);
            let key = PoolKey {
                tag: entry.tag as u8,
                payload: entry.payload.clone(),
            };
            let pool_idx = if let Some(&idx) = self.dedupe.get(&key) {
                idx
            } else {
                let Some(idx) = u32::try_from(self.entries.len()).ok() else {
                    break;
                };
                self.dedupe.insert(key, idx);
                self.entries.push(entry);
                idx
            };
            self.ir_to_pool.push(pool_idx);
        }
    }

    /// Returns the constant pool index for IR literal `literal_index`.
    ///
    /// # Panics
    ///
    /// Panics if `fill_from_ir` was not called or `literal_index` is out of range.
    #[must_use]
    pub fn pool_index_for_literal(&self, literal_index: u32) -> u32 {
        self.ir_to_pool
            .get(literal_index as usize)
            .copied()
            .unwrap_or(literal_index)
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
                PrimitiveKind::F32 => {
                    #[allow(clippy::cast_possible_truncation)]
                    let narrow = *v as f32;
                    narrow.to_le_bytes().to_vec()
                }
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

/// Narrowing/wrapping casts for literal pool bytes (Phoenix `as` semantics).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
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
    fn is_unsigned(&self) -> bool;
}

impl PrimUnsigned for PrimitiveKind {
    fn is_unsigned(&self) -> bool {
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

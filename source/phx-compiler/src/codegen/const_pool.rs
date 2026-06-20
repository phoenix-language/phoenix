//! IR literal → PHX0 constant pool builder.
//!
//! Part of the [`codegen`](crate::codegen) pass. [`ConstPoolBuilder`] converts
//! [`IrConst`](crate::ir::IrConst) values from [`IrModule::constants`](crate::ir::IrModule::constants)
//! into wire-format [`ConstEntry`](phx_bytecode::ConstEntry) payloads, deduplicating identical
//! tag/payload pairs so shared literals occupy one pool slot.
//!
//! Emission ([`super::emit`]) resolves [`IrInst::Const`](crate::ir::IrInst::Const) and
//! [`IrInst::MakeStr`](crate::ir::IrInst::MakeStr) operands through
//! [`ConstPoolBuilder::pool_index_for_literal`], which maps each IR literal index to its pool index.

use std::collections::HashMap;

use phx_bytecode::{ConstEntry, ConstPool, ConstTag, PrimitiveKind};

use crate::codegen::CodegenError;
use crate::codegen::error::u32_section;
use crate::ir::IrConst;

/// Dedupe key: wire tag byte plus encoded payload bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PoolKey {
    tag: u8,
    payload: Vec<u8>,
}

/// Builds a deduplicated module [`ConstPool`] from IR literals.
///
/// Call [`Self::fill_from_ir`] once per module, then look up pool indices while emitting
/// instructions; finish with [`Self::finish`].
#[derive(Debug, Default)]
pub struct ConstPoolBuilder {
    entries: Vec<ConstEntry>,
    dedupe: HashMap<PoolKey, u32>,
    /// IR literal index → constant pool index (parallel to `IrModule::constants`).
    ir_to_pool: Vec<u32>,
}

impl ConstPoolBuilder {
    /// Creates an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers all literals from `constants`, deduplicating identical payloads.
    ///
    /// # Errors
    ///
    /// Returns [`CodegenError::SectionTooLarge`] when the pool exceeds `u32::MAX` entries.
    pub fn fill_from_ir(&mut self, constants: &[IrConst]) -> Result<(), CodegenError> {
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
                let idx = u32_section("const_pool", self.entries.len())?;
                self.dedupe.insert(key, idx);
                self.entries.push(entry);
                idx
            };
            self.ir_to_pool.push(pool_idx);
        }
        Ok(())
    }

    /// Returns the constant pool index for IR literal `literal_index`.
    ///
    /// # Errors
    ///
    /// Returns [`CodegenError::MissingLiteralIndex`] when `literal_index` is out of range
    /// or `fill_from_ir` was not called for that literal.
    pub fn pool_index_for_literal(&self, literal_index: u32) -> Result<u32, CodegenError> {
        self.ir_to_pool
            .get(literal_index as usize)
            .copied()
            .ok_or(CodegenError::MissingLiteralIndex { literal_index })
    }

    /// Consumes the builder and returns the assembled constant pool section.
    #[must_use]
    pub fn finish(self) -> ConstPool {
        ConstPool {
            entries: self.entries,
        }
    }
}

/// Encodes one IR literal as a wire-format constant entry (tag + little-endian payload).
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

/// Encodes an integer literal for `kind`, using Phoenix narrowing/wrapping `as` semantics.
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

#[cfg(test)]
mod tests {
    use super::*;
    use phx_bytecode::PrimitiveKind;

    #[test]
    fn pool_index_for_literal_errors_on_miss() {
        let builder = ConstPoolBuilder::new();
        assert_eq!(
            builder.pool_index_for_literal(0),
            Err(CodegenError::MissingLiteralIndex { literal_index: 0 })
        );
    }

    #[test]
    fn fill_from_ir_maps_literals() {
        let mut builder = ConstPoolBuilder::new();
        let constants = vec![IrConst::Int(1, PrimitiveKind::S32)];
        assert!(builder.fill_from_ir(&constants).is_ok());
        assert_eq!(builder.pool_index_for_literal(0), Ok(0));
    }

    #[test]
    fn pool_index_for_literal_errors_on_out_of_range() {
        let mut builder = ConstPoolBuilder::new();
        assert!(builder.fill_from_ir(&[IrConst::Bool(true)]).is_ok());
        assert_eq!(
            builder.pool_index_for_literal(1),
            Err(CodegenError::MissingLiteralIndex { literal_index: 1 })
        );
    }
}

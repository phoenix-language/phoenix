//! Width-faithful runtime scalar payloads for the MVP stack machine.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use crate::cast::PrimitiveKind;

/// Address tag in the high bits of a raw pointer value.
pub const PTR_LOCAL_TAG: u64 = 0x8000_0000_0000_0000;
/// Address tag for aggregate arena handles used as slice data pointers.
pub const PTR_AGG_TAG: u64 = 0x4000_0000_0000_0000;
/// Address tag for constant-pool indices (`pool_index` in low bits).
pub const PTR_CONST_TAG: u64 = 0x2000_0000_0000_0000;
/// Address tag for function pointer values (`target_kind` in bits 32–39, id in low 32 bits).
pub const PTR_FN_TAG: u64 = 0x1000_0000_0000_0000;

/// A primitive value with storage matching its Phoenix type width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScalarValue {
    /// `bool`
    Bool(bool),
    /// `s8`
    I8(i8),
    /// `s16`
    I16(i16),
    /// `s32`
    I32(i32),
    /// `s64`
    I64(i64),
    /// `s128`
    I128(i128),
    /// `u8`
    U8(u8),
    /// `u16`
    U16(u16),
    /// `u32`
    U32(u32),
    /// `u64`
    U64(u64),
    /// `u128`
    U128(u128),
    /// `f32`
    F32(f32),
    /// `f64`
    F64(f64),
    /// Raw address (`*T`, `&T`, `&mut T`).
    Ptr(u64),
}

impl ScalarValue {
    /// Returns the wire [`PrimitiveKind`] for this scalar, if any.
    #[must_use]
    pub fn primitive_kind(self) -> Option<PrimitiveKind> {
        Some(match self {
            Self::Bool(_) => PrimitiveKind::Bool,
            Self::I8(_) => PrimitiveKind::S8,
            Self::I16(_) => PrimitiveKind::S16,
            Self::I32(_) => PrimitiveKind::S32,
            Self::I64(_) => PrimitiveKind::S64,
            Self::I128(_) => PrimitiveKind::S128,
            Self::U8(_) => PrimitiveKind::U8,
            Self::U16(_) => PrimitiveKind::U16,
            Self::U32(_) => PrimitiveKind::U32,
            Self::U64(_) => PrimitiveKind::U64,
            Self::U128(_) => PrimitiveKind::U128,
            Self::F32(_) => PrimitiveKind::F32,
            Self::F64(_) => PrimitiveKind::F64,
            Self::Ptr(_) => return None,
        })
    }

    /// Zero value for a primitive kind.
    #[must_use]
    pub fn zero(kind: PrimitiveKind) -> Self {
        match kind {
            PrimitiveKind::Bool => Self::Bool(false),
            PrimitiveKind::S8 => Self::I8(0),
            PrimitiveKind::S16 => Self::I16(0),
            PrimitiveKind::S32 => Self::I32(0),
            PrimitiveKind::S64 => Self::I64(0),
            PrimitiveKind::S128 => Self::I128(0),
            PrimitiveKind::U8 => Self::U8(0),
            PrimitiveKind::U16 => Self::U16(0),
            PrimitiveKind::U32 => Self::U32(0),
            PrimitiveKind::U64 => Self::U64(0),
            PrimitiveKind::U128 => Self::U128(0),
            PrimitiveKind::F32 => Self::F32(0.0),
            PrimitiveKind::F64 => Self::F64(0.0),
        }
    }

    /// Encodes a local slot index as a pointer value.
    #[must_use]
    pub fn local_ptr(slot: u32) -> Self {
        Self::Ptr(PTR_LOCAL_TAG | u64::from(slot))
    }

    /// Encodes a function pointer value.
    #[must_use]
    pub fn fn_ptr(target_kind: u32, id: u32) -> Self {
        Self::Ptr(fn_ptr_from_id(target_kind, id))
    }

    /// Decodes a local slot from a local pointer, if tagged correctly.
    #[must_use]
    pub fn local_slot_from_ptr(ptr: u64) -> Option<u32> {
        if ptr & PTR_LOCAL_TAG == PTR_LOCAL_TAG {
            u32::try_from(ptr & !PTR_LOCAL_TAG).ok()
        } else {
            None
        }
    }

    /// Encodes an aggregate handle as a data pointer for slices.
    #[must_use]
    pub fn agg_ptr(handle: u32) -> Self {
        Self::Ptr(PTR_AGG_TAG | u64::from(handle))
    }

    /// Writes little-endian bytes for `kind`.
    #[must_use]
    pub fn to_le_bytes(self, kind: PrimitiveKind) -> Vec<u8> {
        match (kind, self) {
            (PrimitiveKind::Bool, Self::Bool(b)) => vec![u8::from(b)],
            (PrimitiveKind::S8, Self::I8(v)) => vec![v.cast_unsigned()],
            (PrimitiveKind::U8, Self::U8(v)) => vec![v],
            (PrimitiveKind::S16, Self::I16(v)) => v.to_le_bytes().to_vec(),
            (PrimitiveKind::U16, Self::U16(v)) => v.to_le_bytes().to_vec(),
            (PrimitiveKind::S32, Self::I32(v)) => v.to_le_bytes().to_vec(),
            (PrimitiveKind::U32, Self::U32(v)) => v.to_le_bytes().to_vec(),
            (PrimitiveKind::S64, Self::I64(v)) => v.to_le_bytes().to_vec(),
            (PrimitiveKind::U64, Self::U64(v)) => v.to_le_bytes().to_vec(),
            (PrimitiveKind::S128, Self::I128(v)) => v.to_le_bytes().to_vec(),
            (PrimitiveKind::U128, Self::U128(v)) => v.to_le_bytes().to_vec(),
            (PrimitiveKind::F32, Self::F32(v)) => v.to_le_bytes().to_vec(),
            (PrimitiveKind::F64, Self::F64(v)) => v.to_le_bytes().to_vec(),
            _ => Vec::new(),
        }
    }

    /// Decodes from const-pool bytes for `kind`.
    ///
    /// # Errors
    ///
    /// Returns `None` when payload length does not match `kind`.
    #[must_use]
    pub fn from_le_bytes(kind: PrimitiveKind, bytes: &[u8]) -> Option<Self> {
        match kind {
            PrimitiveKind::Bool => {
                let b = bytes.first().copied().unwrap_or(0);
                Some(Self::Bool(b != 0))
            }
            PrimitiveKind::S8 => Some(Self::I8((*bytes.first()?).cast_signed())),
            PrimitiveKind::U8 => Some(Self::U8(*bytes.first()?)),
            PrimitiveKind::S16 => {
                let b: [u8; 2] = bytes.get(0..2)?.try_into().ok()?;
                Some(Self::I16(i16::from_le_bytes(b)))
            }
            PrimitiveKind::U16 => {
                let b: [u8; 2] = bytes.get(0..2)?.try_into().ok()?;
                Some(Self::U16(u16::from_le_bytes(b)))
            }
            PrimitiveKind::S32 => {
                let b: [u8; 4] = bytes.get(0..4)?.try_into().ok()?;
                Some(Self::I32(i32::from_le_bytes(b)))
            }
            PrimitiveKind::U32 => {
                let b: [u8; 4] = bytes.get(0..4)?.try_into().ok()?;
                Some(Self::U32(u32::from_le_bytes(b)))
            }
            PrimitiveKind::S64 => {
                let b: [u8; 8] = bytes.get(0..8)?.try_into().ok()?;
                Some(Self::I64(i64::from_le_bytes(b)))
            }
            PrimitiveKind::U64 => {
                let b: [u8; 8] = bytes.get(0..8)?.try_into().ok()?;
                Some(Self::U64(u64::from_le_bytes(b)))
            }
            PrimitiveKind::S128 => {
                let b: [u8; 16] = bytes.get(0..16)?.try_into().ok()?;
                Some(Self::I128(i128::from_le_bytes(b)))
            }
            PrimitiveKind::U128 => {
                let b: [u8; 16] = bytes.get(0..16)?.try_into().ok()?;
                Some(Self::U128(u128::from_le_bytes(b)))
            }
            PrimitiveKind::F32 => {
                let b: [u8; 4] = bytes.get(0..4)?.try_into().ok()?;
                Some(Self::F32(f32::from_le_bytes(b)))
            }
            PrimitiveKind::F64 => {
                let b: [u8; 8] = bytes.get(0..8)?.try_into().ok()?;
                Some(Self::F64(f64::from_le_bytes(b)))
            }
        }
    }

    /// Returns `true` when the value is truthy (for branches).
    #[must_use]
    pub fn is_truthy(self) -> bool {
        match self {
            Self::Bool(b) => b,
            Self::Ptr(p) => p != 0,
            Self::I8(v) => v != 0,
            Self::I16(v) => v != 0,
            Self::I32(v) => v != 0,
            Self::I64(v) => v != 0,
            Self::I128(v) => v != 0,
            Self::U8(v) => v != 0,
            Self::U16(v) => v != 0,
            Self::U32(v) => v != 0,
            Self::U64(v) => v != 0,
            Self::U128(v) => v != 0,
            Self::F32(v) => v != 0.0,
            Self::F64(v) => v != 0.0,
        }
    }
}

/// Encodes a function pointer raw address.
///
/// `target_kind` `0` = Phoenix `function_id`; `1` = foreign stub id.
#[must_use]
pub fn fn_ptr_from_id(target_kind: u32, id: u32) -> u64 {
    PTR_FN_TAG | ((u64::from(target_kind) & 0xFF) << 32) | u64::from(id)
}

/// Decodes `(target_kind, id)` from a function pointer raw address.
#[must_use]
pub fn decode_fn_ptr(ptr: u64) -> (u32, u32) {
    let id = u32::try_from(ptr & 0xFFFF_FFFF).unwrap_or(0);
    let target_kind = u32::try_from((ptr >> 32) & 0xFF).unwrap_or(0);
    (target_kind, id)
}

/// Returns `true` when `ptr` carries the function-pointer tag.
#[must_use]
pub fn is_fn_ptr(ptr: u64) -> bool {
    ptr & PTR_FN_TAG == PTR_FN_TAG
}

/// Widens a scalar to `i128` using sign extension for signed storage variants.
#[must_use]
pub fn scalar_to_i128(value: ScalarValue, _from: PrimitiveKind) -> i128 {
    match value {
        ScalarValue::I8(v) => i128::from(v),
        ScalarValue::I16(v) => i128::from(v),
        ScalarValue::I32(v) => i128::from(v),
        ScalarValue::I64(v) => i128::from(v),
        ScalarValue::I128(v) => v,
        ScalarValue::U8(v) => i128::from(v),
        ScalarValue::U16(v) => i128::from(v),
        ScalarValue::U32(v) => i128::from(v),
        ScalarValue::U64(v) => i128::from(v),
        ScalarValue::U128(v) => v as i128,
        ScalarValue::Bool(b) => i128::from(b),
        ScalarValue::F32(v) => f64::from(v) as i128,
        ScalarValue::F64(v) => v as i128,
        ScalarValue::Ptr(p) => i128::from(p),
    }
}

/// Widens a scalar to `u128` using zero extension for unsigned storage variants.
#[must_use]
pub fn scalar_to_u128(value: ScalarValue, _from: PrimitiveKind) -> u128 {
    match value {
        ScalarValue::U8(v) => u128::from(v),
        ScalarValue::U16(v) => u128::from(v),
        ScalarValue::U32(v) => u128::from(v),
        ScalarValue::U64(v) => u128::from(v),
        ScalarValue::U128(v) => v,
        ScalarValue::I8(v) => u128::from(v.cast_unsigned()),
        ScalarValue::I16(v) => u128::from(v.cast_unsigned()),
        ScalarValue::I32(v) => u128::from(v.cast_unsigned()),
        ScalarValue::I64(v) => u128::from(v.cast_unsigned()),
        ScalarValue::I128(v) => v.cast_unsigned(),
        ScalarValue::Bool(b) => u128::from(b),
        ScalarValue::F32(v) => f64::from(v) as u128,
        ScalarValue::F64(v) => v as u128,
        ScalarValue::Ptr(p) => u128::from(p),
    }
}

/// Narrows an `i128` to `kind` with truncating/wrapping casts.
#[must_use]
pub fn scalar_from_i128(value: i128, kind: PrimitiveKind) -> ScalarValue {
    match kind {
        PrimitiveKind::S8 => ScalarValue::I8(value as i8),
        PrimitiveKind::S16 => ScalarValue::I16(value as i16),
        PrimitiveKind::S32 => ScalarValue::I32(value as i32),
        PrimitiveKind::S64 => ScalarValue::I64(value as i64),
        PrimitiveKind::S128 => ScalarValue::I128(value),
        PrimitiveKind::U8 => ScalarValue::U8(value as u8),
        PrimitiveKind::U16 => ScalarValue::U16(value as u16),
        PrimitiveKind::U32 => ScalarValue::U32(value as u32),
        PrimitiveKind::U64 => ScalarValue::U64(value as u64),
        PrimitiveKind::U128 => ScalarValue::U128(value as u128),
        PrimitiveKind::Bool => ScalarValue::Bool(value != 0),
        PrimitiveKind::F32 => ScalarValue::F32(value as f32),
        PrimitiveKind::F64 => ScalarValue::F64(value as f64),
    }
}

/// Narrows a `u128` to `kind` with truncating/wrapping casts.
#[must_use]
pub fn scalar_from_u128(value: u128, kind: PrimitiveKind) -> ScalarValue {
    match kind {
        PrimitiveKind::S8 => ScalarValue::I8(value as i8),
        PrimitiveKind::S16 => ScalarValue::I16(value as i16),
        PrimitiveKind::S32 => ScalarValue::I32(value as i32),
        PrimitiveKind::S64 => ScalarValue::I64(value as i64),
        PrimitiveKind::S128 => ScalarValue::I128(value as i128),
        PrimitiveKind::U8 => ScalarValue::U8(value as u8),
        PrimitiveKind::U16 => ScalarValue::U16(value as u16),
        PrimitiveKind::U32 => ScalarValue::U32(value as u32),
        PrimitiveKind::U64 => ScalarValue::U64(value as u64),
        PrimitiveKind::U128 => ScalarValue::U128(value),
        PrimitiveKind::Bool => ScalarValue::Bool(value != 0),
        PrimitiveKind::F32 => ScalarValue::F32(value as f32),
        PrimitiveKind::F64 => ScalarValue::F64(value as f64),
    }
}

/// Widens a scalar to `f64` for float casts and arithmetic.
#[must_use]
pub fn scalar_to_f64(value: ScalarValue, from: PrimitiveKind) -> f64 {
    match value {
        ScalarValue::F32(v) => f64::from(v),
        ScalarValue::F64(v) => v,
        other if from.is_unsigned_int() => scalar_to_u128(other, from) as f64,
        other => scalar_to_i128(other, from) as f64,
    }
}

/// Masks a shift amount to the operand bit width (Rust/Wasm `wrapping_shl` on declared width).
///
/// Equivalent to `amount & (W - 1)` when `W` is a power of two.
#[must_use]
pub fn mask_shift_amount(amount: u128, kind: PrimitiveKind) -> u32 {
    let width = u128::from(kind.bit_width());
    debug_assert!(
        width > 0,
        "shift masking requires an integer primitive kind"
    );
    (amount % width) as u32
}

/// Narrows an `f64` to `kind`.
#[must_use]
pub fn scalar_from_f64(value: f64, kind: PrimitiveKind) -> ScalarValue {
    match kind {
        PrimitiveKind::F32 => ScalarValue::F32(value as f32),
        PrimitiveKind::F64 => ScalarValue::F64(value),
        _ if kind.is_unsigned_int() => scalar_from_u128(value as u128, kind),
        _ => scalar_from_i128(value as i128, kind),
    }
}

#[cfg(test)]
mod shift_mask_tests {
    use super::*;

    #[test]
    fn mask_u8_shift_9() {
        assert_eq!(mask_shift_amount(9, PrimitiveKind::U8), 1);
    }

    #[test]
    fn mask_u32_shift_32() {
        assert_eq!(mask_shift_amount(32, PrimitiveKind::U32), 0);
    }
}

#[cfg(test)]
mod fn_ptr_tests {
    use super::*;

    #[test]
    fn fn_ptr_roundtrip() {
        let ptr = fn_ptr_from_id(0, 42);
        assert!(is_fn_ptr(ptr));
        assert_eq!(decode_fn_ptr(ptr), (0, 42));
    }

    #[test]
    fn foreign_fn_ptr_roundtrip() {
        let ptr = fn_ptr_from_id(1, 7);
        assert_eq!(decode_fn_ptr(ptr), (1, 7));
    }
}

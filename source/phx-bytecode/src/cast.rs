//! Primitive cast targets shared by codegen and VM.

use crate::scalar::ScalarValue;

/// Wire encoding for explicit `as` cast operands (stable per format version).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PrimitiveKind {
    /// `s8`
    S8 = 0,
    /// `s16`
    S16 = 1,
    /// `s32`
    S32 = 2,
    /// `s64`
    S64 = 3,
    /// `s128`
    S128 = 4,
    /// `u8`
    U8 = 5,
    /// `u16`
    U16 = 6,
    /// `u32`
    U32 = 7,
    /// `u64`
    U64 = 8,
    /// `u128`
    U128 = 9,
    /// `bool`
    Bool = 10,
    /// `f32`
    F32 = 11,
    /// `f64`
    F64 = 12,
}

/// Wire tag for non-primitive local slots (aggregates).
pub const SLOT_KIND_AGG: u8 = 0xFF;

impl PrimitiveKind {
    /// Decodes a cast operand.
    #[must_use]
    pub fn from_u8(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::S8),
            1 => Some(Self::S16),
            2 => Some(Self::S32),
            3 => Some(Self::S64),
            4 => Some(Self::S128),
            5 => Some(Self::U8),
            6 => Some(Self::U16),
            7 => Some(Self::U32),
            8 => Some(Self::U64),
            9 => Some(Self::U128),
            10 => Some(Self::Bool),
            11 => Some(Self::F32),
            12 => Some(Self::F64),
            _ => None,
        }
    }

    /// Returns the wire discriminant.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Returns `true` for floating-point kinds.
    #[must_use]
    pub const fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    /// Storage size in bytes for this primitive.
    #[must_use]
    pub const fn byte_size(self) -> u8 {
        match self {
            Self::Bool | Self::S8 | Self::U8 => 1,
            Self::S16 | Self::U16 => 2,
            Self::S32 | Self::U32 | Self::F32 => 4,
            Self::S64 | Self::U64 | Self::F64 => 8,
            Self::S128 | Self::U128 => 16,
        }
    }

    /// Bit width for integer kinds.
    #[must_use]
    pub const fn bit_width(self) -> u32 {
        match self.byte_size() {
            1 => 8,
            2 => 16,
            4 => 32,
            8 => 64,
            16 => 128,
            _ => 0,
        }
    }

    #[must_use]
    const fn is_unsigned(self) -> bool {
        matches!(
            self,
            Self::U8 | Self::U16 | Self::U32 | Self::U64 | Self::U128
        )
    }

    /// Applies an explicit cast from `from` representation to `to`.
    #[must_use]
    pub fn apply_cast(value: ScalarValue, from: Self, to: Self) -> ScalarValue {
        if from == to {
            return value;
        }
        if to == Self::Bool {
            return ScalarValue::Bool(value.is_truthy());
        }
        if from == Self::Bool {
            let b = value.is_truthy();
            return Self::apply_cast(ScalarValue::I32(i32::from(b)), Self::S32, to);
        }
        if from.is_float() || to.is_float() {
            let f = scalar_to_f64(value, from);
            return scalar_from_f64(f, to);
        }
        let wide = scalar_to_i128(value, from);
        scalar_from_i128(wide, to)
    }
}

fn scalar_to_i128(value: ScalarValue, _from: PrimitiveKind) -> i128 {
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
        ScalarValue::Ptr(p) => i128::try_from(p).unwrap_or(0),
    }
}

fn scalar_to_f64(value: ScalarValue, from: PrimitiveKind) -> f64 {
    match value {
        ScalarValue::F32(v) => f64::from(v),
        ScalarValue::F64(v) => v,
        other => scalar_to_i128(other, from) as f64,
    }
}

fn scalar_from_i128(value: i128, to: PrimitiveKind) -> ScalarValue {
    match to {
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

fn scalar_from_f64(value: f64, to: PrimitiveKind) -> ScalarValue {
    match to {
        PrimitiveKind::F32 => ScalarValue::F32(value as f32),
        PrimitiveKind::F64 => ScalarValue::F64(value),
        _ => scalar_from_i128(value as i128, to),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cast_u32_to_u8_masks() {
        assert_eq!(
            PrimitiveKind::apply_cast(
                ScalarValue::U32(300),
                PrimitiveKind::U32,
                PrimitiveKind::U8
            ),
            ScalarValue::U8(44)
        );
    }

    #[test]
    fn cast_s32_to_s64_extends() {
        assert_eq!(
            PrimitiveKind::apply_cast(
                ScalarValue::I32(-1),
                PrimitiveKind::S32,
                PrimitiveKind::S64
            ),
            ScalarValue::I64(-1)
        );
    }

    #[test]
    fn cast_i128_roundtrip() {
        let v = ScalarValue::I128(1_000_000_000_000_000_000);
        let narrowed = PrimitiveKind::apply_cast(v, PrimitiveKind::S128, PrimitiveKind::S64);
        assert_eq!(narrowed, ScalarValue::I64(1_000_000_000_000_000_000));
    }
}

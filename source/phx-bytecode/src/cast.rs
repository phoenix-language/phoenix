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

impl PrimitiveKind {
    /// Decodes a cast operand.
    ///
    /// # Errors
    ///
    /// Returns `None` for unrecognized bytes.
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

    /// Bit width for integer kinds; `bool` is 1 bit stored as one byte.
    #[must_use]
    pub const fn bit_width(self) -> u32 {
        match self {
            Self::S8 | Self::U8 | Self::Bool => 8,
            Self::S16 | Self::U16 => 16,
            Self::S32 | Self::U32 | Self::F32 => 32,
            Self::S64 | Self::U64 | Self::F64 => 64,
            Self::S128 | Self::U128 => 128,
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
            let n = match value {
                ScalarValue::Int(v) => v != 0,
                ScalarValue::Float(v) => v != 0.0,
            };
            return ScalarValue::Int(i64::from(n));
        }
        if from == Self::Bool {
            let raw = value.as_int().unwrap_or(0);
            return Self::apply_cast(ScalarValue::Int(raw), Self::S32, to);
        }
        if from.is_float() || to.is_float() {
            let f = scalar_to_f64(value, from);
            return scalar_from_f64(f, to);
        }
        let raw = value.as_int().unwrap_or(0);
        let masked = if from.is_unsigned() {
            mask_unsigned(raw, from.bit_width()) as i64
        } else {
            sign_extend(trunc_bits(raw, from.bit_width()), from.bit_width())
        };
        if to.is_unsigned() {
            ScalarValue::Int(mask_unsigned(masked, to.bit_width()) as i64)
        } else {
            ScalarValue::Int(sign_extend(trunc_bits(masked, to.bit_width()), to.bit_width()))
        }
    }
}

fn scalar_to_f64(value: ScalarValue, from: PrimitiveKind) -> f64 {
    match value {
        ScalarValue::Float(v) => {
            if from == PrimitiveKind::F32 {
                (v as f32) as f64
            } else {
                v
            }
        }
        ScalarValue::Int(v) => {
            if from.is_unsigned() {
                mask_unsigned(v, from.bit_width()) as f64
            } else {
                sign_extend(trunc_bits(v, from.bit_width()), from.bit_width()) as f64
            }
        }
    }
}

fn scalar_from_f64(value: f64, to: PrimitiveKind) -> ScalarValue {
    match to {
        PrimitiveKind::F32 => ScalarValue::Float(f64::from(value as f32)),
        PrimitiveKind::F64 => ScalarValue::Float(value),
        PrimitiveKind::Bool => ScalarValue::Int(i64::from(value != 0.0)),
        _ if to.is_unsigned() => {
            let bits = if to.bit_width() >= 64 {
                value as u64
            } else {
                (value as u64) & ((1u64 << to.bit_width()) - 1)
            };
            ScalarValue::Int(bits as i64)
        }
        _ => ScalarValue::Int(value as i64),
    }
}

const fn trunc_bits(value: i64, width: u32) -> u64 {
    let mask = if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    (value as u64) & mask
}

const fn mask_unsigned(value: i64, width: u32) -> u64 {
    trunc_bits(value, width)
}

const fn sign_extend(value: u64, width: u32) -> i64 {
    if width >= 64 {
        return value as i64;
    }
    let sign = 1u64 << (width - 1);
    if value & sign != 0 {
        (value | (!0u64 << width)) as i64
    } else {
        value as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cast_u32_to_u8_masks() {
        assert_eq!(
            PrimitiveKind::apply_cast(
                ScalarValue::Int(300),
                PrimitiveKind::U32,
                PrimitiveKind::U8
            ),
            ScalarValue::Int(44)
        );
    }

    #[test]
    fn cast_s32_to_s64_extends() {
        assert_eq!(
            PrimitiveKind::apply_cast(
                ScalarValue::Int(-1),
                PrimitiveKind::S32,
                PrimitiveKind::S64
            ),
            ScalarValue::Int(-1)
        );
    }

    #[test]
    fn cast_int_to_float() {
        let v = PrimitiveKind::apply_cast(
            ScalarValue::Int(3),
            PrimitiveKind::S32,
            PrimitiveKind::F64,
        );
        assert_eq!(v, ScalarValue::Float(3.0));
    }

    #[test]
    fn cast_float_to_int() {
        let v = PrimitiveKind::apply_cast(
            ScalarValue::Float(2.9),
            PrimitiveKind::F64,
            PrimitiveKind::S32,
        );
        assert_eq!(v, ScalarValue::Int(2));
    }
}

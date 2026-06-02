//! Primitive cast targets shared by codegen and VM.

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
            _ => None,
        }
    }

    /// Returns the wire discriminant.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Bit width for integer kinds; `bool` is 1 bit stored as one byte.
    #[must_use]
    pub const fn bit_width(self) -> u32 {
        match self {
            Self::S8 | Self::U8 | Self::Bool => 8,
            Self::S16 | Self::U16 => 16,
            Self::S32 | Self::U32 => 32,
            Self::S64 | Self::U64 => 64,
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
    pub fn apply_cast(value: i64, from: Self, to: Self) -> i64 {
        if from == to {
            return value;
        }
        if to == Self::Bool {
            return i64::from(value != 0);
        }
        if from == Self::Bool {
            return if value != 0 { 1 } else { 0 };
        }
        let raw = if from.is_unsigned() {
            mask_unsigned(value, from.bit_width()) as i64
        } else {
            sign_extend(trunc_bits(value, from.bit_width()), from.bit_width())
        };
        if to.is_unsigned() {
            mask_unsigned(raw, to.bit_width()) as i64
        } else {
            sign_extend(trunc_bits(raw, to.bit_width()), to.bit_width())
        }
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
            PrimitiveKind::apply_cast(300, PrimitiveKind::U32, PrimitiveKind::U8),
            44
        );
    }

    #[test]
    fn cast_s32_to_s64_extends() {
        assert_eq!(
            PrimitiveKind::apply_cast(-1, PrimitiveKind::S32, PrimitiveKind::S64),
            -1
        );
    }
}

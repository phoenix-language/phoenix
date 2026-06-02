//! Runtime scalar payloads for the MVP stack machine.

/// A primitive value on the operand stack (integers, floats, bool as int).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScalarValue {
    /// Signed integer lane (`s8`…`s128`, `u8`…`u128`, `bool` as 0/1).
    Int(i64),
    /// `f32` / `f64` (stored as `f64`; narrowing happens in cast).
    Float(f64),
}

impl ScalarValue {
    /// Returns the integer lane or `None` for floats.
    #[must_use]
    pub const fn as_int(self) -> Option<i64> {
        match self {
            Self::Int(v) => Some(v),
            Self::Float(_) => None,
        }
    }

    /// Returns the float lane or `None` for integers.
    #[must_use]
    pub fn as_float(self) -> Option<f64> {
        match self {
            Self::Float(v) => Some(v),
            Self::Int(_) => None,
        }
    }

    /// Zero integer scalar.
    #[must_use]
    pub const fn zero_int() -> Self {
        Self::Int(0)
    }
}

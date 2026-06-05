//! Literal AST types.
//!
//! Literal values attached to expressions and patterns (numeric, bool, byte char/string).

use crate::token::{FloatSuffix, IntegerSuffix};

/// An integer literal in the AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntLit {
    /// Parsed value.
    pub value: i128,
    /// Optional `u` suffix.
    pub suffix: IntegerSuffix,
}

/// A floating literal in the AST.
#[derive(Debug, Clone, PartialEq)]
pub struct FloatLit {
    /// Parsed value.
    pub value: f64,
    /// Optional type suffix.
    pub suffix: FloatSuffix,
}

/// A literal expression payload.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Literal {
    /// Integer literal.
    Int(IntLit),
    /// Float literal.
    Float(FloatLit),
    /// `true` or `false`.
    Bool(bool),
    /// `b'…'`.
    ByteChar(u8),
    /// `b"…"`.
    ByteString(Vec<u8>),
    /// `"…"` (validated UTF-8).
    String(String),
}

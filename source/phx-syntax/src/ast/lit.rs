//! Literal AST types.
//!
//! Literal payloads shared by expressions ([`super::expr::Expr::Literal`]) and patterns
//! ([`super::Pattern::Literal`]). Numeric literals record optional suffixes from
//! [`crate::token`]; the type checker interprets default types.
//!
//! ## Payloads
//!
//! - [`IntLit`] / [`FloatLit`] — parsed numeric value plus suffix.
//! - [`Literal::Bool`], [`Literal::ByteChar`], [`Literal::ByteStr`], [`Literal::Unit`] — non-numeric
//!   literals.

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

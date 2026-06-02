//! IR constant literals (lowered before bytecode pool emission).

use phx_bytecode::PrimitiveKind;

/// A module-level constant literal referenced by [`super::IrInst::Const`].
#[derive(Debug, Clone, PartialEq)]
pub enum IrConst {
    /// Signed integer constant with target width.
    Int(i128, PrimitiveKind),
    /// Floating constant with target width.
    Float(f64, PrimitiveKind),
    /// Boolean constant.
    Bool(bool),
    /// Raw byte blob (e.g. byte string before `MakeArray`).
    Bytes(Vec<u8>),
}

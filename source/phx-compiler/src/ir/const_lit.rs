//! IR constant literals (lowered before bytecode pool emission).

/// A module-level constant literal referenced by [`super::IrInst::Const`].
#[derive(Debug, Clone, PartialEq)]
pub enum IrConst {
    /// Signed integer constant.
    Int(i64),
    /// Floating constant (`f32`/`f64`).
    Float(f64),
    /// Boolean constant.
    Bool(bool),
}

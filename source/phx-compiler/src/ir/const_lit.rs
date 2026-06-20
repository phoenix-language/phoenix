//! IR constant literals (lowered before bytecode pool emission).
//!
//! [`IrConst`] entries populate [`IrModule::constants`](super::IrModule::constants). Lowering
//! interns literals via `LowerCtx::intern_const` and references them from [`IrInst::Const`](super::inst::IrInst)
//! by dense index. Codegen translates each entry into the bytecode constant pool.
//!
//! ## Variants
//!
//! - Numeric and boolean literals carry their target [`PrimitiveKind`].
//! - [`IrConst::Bytes`] holds raw byte blobs for byte strings and interned UTF-8 string payloads
//!   before [`IrInst::MakeStr`](super::inst::IrInst) / array construction.
//!
//! ## Invariants
//!
//! - Pool indices are stable for the lifetime of an [`IrModule`]; [`IrInst::Const`] operands are
//!   `u32` indices into [`IrModule::constants`].

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

//! IR instructions paired with source spans for backend diagnostics.
//!
//! Lowering wraps every [`IrInst`](super::inst::IrInst) in [`SpannedInst`] so validation, codegen,
//! and dev tooling can attribute failures to the originating Phoenix source. Spans are preserved
//! through the IR pipeline and serialized to PHX0 section 5 (PC span map) in dev builds; see
//! [`debug.md`](../../../docs/design/features/debug.md).
//!
//! ## Invariants
//!
//! - Every instruction emitted by lowering carries the span of the expression, statement, or
//!   desugared construct that produced it.
//! - Span data is diagnostic-only; it does not affect bytecode semantics or VM execution.

use phx_diagnostics::Span;

use super::inst::IrInst;

/// One IR instruction with the source span of the construct that produced it.
///
/// Spans are used for lowering/codegen/validation diagnostics; dev builds serialize them to
/// PHX0 section 5 (PC span map — see `debug.md`).
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedInst {
    /// Source span of the expression, statement, or desugared construct.
    pub span: Span,
    /// Instruction payload.
    pub inst: IrInst,
}

impl SpannedInst {
    /// Creates a spanned instruction.
    #[must_use]
    pub const fn new(span: Span, inst: IrInst) -> Self {
        Self { span, inst }
    }
}

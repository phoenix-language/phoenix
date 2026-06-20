//! IR instructions paired with source spans for backend diagnostics.
//!
//! **Pipeline position:** after lowering, before codegen. Every [`IrInst`](super::inst::IrInst)
//! stored in an [`IrBasicBlock`](super::block::IrBasicBlock) is wrapped in [`SpannedInst`] so
//! validation, codegen, and dev tooling can attribute failures to the originating Phoenix source.
//!
//! **Inputs:** [`IrInst`] payloads and the [`Span`](phx_diagnostics::Span) of the expression,
//! statement, or desugared construct that produced each instruction.
//!
//! **Outputs:** span-tagged instructions consumed by [`validate_function`](super::validate_function),
//! [`crate::codegen::codegen_module`], and (in dev builds) PHX0 section 5 (PC span map); see
//! [`debug.md`](../../../docs/design/features/debug.md).
//!
//! ## Public API
//!
//! | Type | Role |
//! |---|---|
//! | [`SpannedInst`] | One instruction plus its diagnostic source span |
//! | [`SpannedInst::new`] | Construct a spanned instruction at lowering sites |
//!
//! ## Invariants
//!
//! - Every instruction emitted by lowering carries the span of the expression, statement, or
//!   desugared construct that produced it.
//! - Span data is diagnostic-only; it does not affect bytecode semantics or VM execution.
//! - [`Self::span`] may cover a wider syntactic region than a single token when lowering
//!   desugars control flow (for example `if` expressions and short-circuit `&&` / `||`).

use phx_diagnostics::Span;

use super::inst::IrInst;

/// One IR instruction with the source span of the construct that produced it.
///
/// Lowering stores these in [`IrBasicBlock::insts`](super::block::IrBasicBlock::insts). Validation
/// and codegen read [`Self::inst`] for semantics and [`Self::span`] only when emitting diagnostics
/// or dev-only PHX0 metadata.
///
/// Re-exported from [`crate::ir`] and [`crate::unstable`] for contributor tooling that inspects
/// lowered graphs; embedders should use [`crate::facade::compile_to_module`] rather than holding
/// IR across releases.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedInst {
    /// Source span of the expression, statement, or desugared construct.
    ///
    /// Used for backend error attribution and dev PC span maps; ignored by the VM at runtime.
    pub span: Span,
    /// Instruction payload evaluated by codegen and stack simulation.
    pub inst: IrInst,
}

impl SpannedInst {
    /// Wraps `inst` with `span` for storage in an [`IrBasicBlock`](super::block::IrBasicBlock).
    ///
    /// Prefer the span of the innermost user-written construct when lowering desugars control flow
    /// so diagnostics point at familiar source locations.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub const fn new(span: Span, inst: IrInst) -> Self {
        Self { span, inst }
    }
}

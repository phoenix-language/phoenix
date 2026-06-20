//! IR basic blocks in Phoenix CFGs.
//!
//! Each [`IrFunction`](super::func::IrFunction) owns a `Vec` of [`IrBasicBlock`]s forming its
//! control-flow graph. Block `0` is the entry; [`IrBasicBlock::insts`] holds a straight-line
//! sequence of [`SpannedInst`](super::SpannedInst) ending in a terminator or implicit fallthrough.
//!
//! Codegen maps each block to a bytecode basic block; [`validate_function`](super::validate_function)
//! checks terminator placement, jump targets, and operand-stack depth at merge points before
//! emission when validation is enabled.
//!
//! ## Invariants
//!
//! - At most one terminator per block; no instructions may follow a terminator.
//! - Jump targets must refer to in-range block indices; loop exits are patched by
//!   [`crate::lower::LowerCtx::patch_loop_exit_targets`].
//! - A block without an explicit terminator may fall through to the next block only when that
//!   successor exists (matching codegen layout).

use super::spanned::SpannedInst;

/// A straight-line sequence of instructions ending in a terminator.
///
/// The IR validator (debug builds and tests) enforces that each block has at most
/// one terminator, that jump targets are in range, and that blocks without a terminator may
/// fall through to the next block only when that successor exists (matching codegen).
#[derive(Debug, Clone, Default)]
pub struct IrBasicBlock {
    /// Instructions in order; the last non-fallthrough block must end in a terminator
    /// (`Return`, `Jump`, `JumpIf`, or `TrapGivenMismatch`).
    pub insts: Vec<SpannedInst>,
}

impl IrBasicBlock {
    /// Creates an empty block.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

//! IR basic blocks.

use super::inst::IrInst;

/// A straight-line sequence of instructions ending in a terminator.
///
/// The IR validator (debug builds and tests) enforces that each block has at most
/// one terminator, that jump targets are in range, and that blocks without a terminator may
/// fall through to the next block only when that successor exists (matching codegen).
#[derive(Debug, Clone, Default)]
pub struct IrBasicBlock {
    /// Instructions in order; the last non-fallthrough block must end in a terminator
    /// (`Return`, `Jump`, `JumpIf`, or `TrapGivenMismatch`).
    pub insts: Vec<IrInst>,
}

impl IrBasicBlock {
    /// Creates an empty block.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

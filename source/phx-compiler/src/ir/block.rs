//! IR basic blocks.

use super::inst::IrInst;

/// A straight-line sequence of instructions ending in a terminator.
#[derive(Debug, Clone, Default)]
pub struct IrBasicBlock {
    /// Instructions in order; the last must be a terminator (`Return`, `Jump`, `JumpIf`).
    pub insts: Vec<IrInst>,
}

impl IrBasicBlock {
    /// Creates an empty block.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

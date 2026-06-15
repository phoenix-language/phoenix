//! IR validation failure types (internal compiler invariant violations).

use core::fmt;

use crate::LocatedError;
use crate::Span;
use crate::code::DiagnosticCode;

/// An IR validation error when lowered CFG or stack discipline is inconsistent.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IrError {
    /// Function has no basic blocks.
    EmptyFunction {
        /// Lowered function definition index.
        def_index: u32,
    },
    /// Block ends without a terminator and has no fallthrough successor.
    MissingTerminator {
        /// Lowered function definition index.
        def_index: u32,
        /// Basic block index.
        block: u32,
    },
    /// Instruction appears after a control-flow terminator.
    InstructionAfterTerminator {
        /// Lowered function definition index.
        def_index: u32,
        /// Basic block index.
        block: u32,
        /// Index of the instruction after the terminator.
        inst_index: u32,
        /// Source span of the offending instruction.
        span: Span,
    },
    /// Jump target is out of range for the function's block list.
    InvalidJumpTarget {
        /// Lowered function definition index.
        def_index: u32,
        /// Basic block containing the jump.
        block: u32,
        /// Invalid target block index.
        target: u32,
        /// Number of basic blocks in the function.
        block_count: u32,
    },
    /// Loop exit placeholder was not patched by lowering.
    UnpatchedLoopExit {
        /// Lowered function definition index.
        def_index: u32,
        /// Basic block containing the jump.
        block: u32,
        /// Unpatched placeholder target.
        target: u32,
        /// Source span of the jump instruction.
        span: Span,
    },
    /// Simulated stack depth would underflow before an instruction.
    StackUnderflow {
        /// Lowered function definition index.
        def_index: u32,
        /// Basic block index.
        block: u32,
        /// Instruction index within the block.
        inst_index: u32,
        /// Stack depth before the instruction.
        depth: u32,
        /// Source span of the faulting instruction.
        span: Span,
    },
    /// Same block reached on two CFG paths with different entry stack depths.
    JoinDepthMismatch {
        /// Lowered function definition index.
        def_index: u32,
        /// Merge basic block index.
        block: u32,
        /// Previously recorded entry depth.
        expected: u32,
        /// Depth on the conflicting incoming edge.
        found: u32,
        /// Source span of the jump that exposed the mismatch.
        span: Span,
    },
}

impl IrError {
    /// Stable diagnostic code for this error.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        DiagnosticCode::new("E4002")
    }

    /// Source span when available.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        match self {
            Self::InstructionAfterTerminator { span, .. }
            | Self::UnpatchedLoopExit { span, .. }
            | Self::StackUnderflow { span, .. }
            | Self::JoinDepthMismatch { span, .. } => Some(*span),
            Self::EmptyFunction { .. }
            | Self::MissingTerminator { .. }
            | Self::InvalidJumpTarget { .. } => None,
        }
    }
}

impl fmt::Display for IrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyFunction { def_index } => {
                write!(
                    f,
                    "internal error: function def {def_index} has no basic blocks after lowering"
                )
            }
            Self::MissingTerminator { def_index, block } => {
                write!(
                    f,
                    "internal error: function def {def_index} block {block} missing terminator"
                )
            }
            Self::InstructionAfterTerminator {
                def_index,
                block,
                inst_index,
                ..
            } => {
                write!(
                    f,
                    "internal error: function def {def_index} block {block} has instruction after terminator at index {inst_index}"
                )
            }
            Self::InvalidJumpTarget {
                def_index,
                block,
                target,
                block_count,
            } => {
                write!(
                    f,
                    "internal error: function def {def_index} block {block} jumps to invalid block {target} (block count {block_count})"
                )
            }
            Self::UnpatchedLoopExit {
                def_index,
                block,
                target,
                ..
            } => {
                write!(
                    f,
                    "internal error: function def {def_index} block {block} has unpatched loop exit target {target:#x}"
                )
            }
            Self::StackUnderflow {
                def_index,
                block,
                inst_index,
                depth,
                ..
            } => {
                write!(
                    f,
                    "internal error: function def {def_index} block {block} instruction {inst_index} stack underflow at depth {depth}"
                )
            }
            Self::JoinDepthMismatch {
                def_index,
                block,
                expected,
                found,
                ..
            } => {
                write!(
                    f,
                    "internal error: function def {def_index} block {block} join stack depth mismatch (expected {expected}, found {found})"
                )
            }
        }
    }
}

impl std::error::Error for IrError {}

/// Result of IR validation when errors may be collected.
pub type IrResult<T> = Result<T, IrBag>;

/// Collected IR validation diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IrBag {
    errors: Vec<LocatedError<IrError>>,
}

impl IrBag {
    /// Creates an empty bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an error for `module`.
    pub fn push(&mut self, module: u32, error: IrError) {
        self.errors.push(LocatedError::new(module, error));
    }

    /// Returns true when at least one error was recorded.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Borrows collected errors.
    #[must_use]
    pub fn errors(&self) -> &[LocatedError<IrError>] {
        &self.errors
    }

    /// Consumes the bag into a flat error list.
    #[must_use]
    pub fn into_errors(self) -> Vec<LocatedError<IrError>> {
        self.errors
    }
}

impl std::fmt::Display for IrBag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, located) in self.errors.iter().enumerate() {
            if i > 0 {
                f.write_str("\n---\n")?;
            }
            write!(f, "{}", located.error)?;
        }
        Ok(())
    }
}

impl std::error::Error for IrBag {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_terminator_display_and_code() {
        let err = IrError::MissingTerminator {
            def_index: 3,
            block: 1,
        };
        assert_eq!(err.code(), DiagnosticCode::new("E4002"));
        assert!(err.to_string().contains("def 3"), "{}", err);
        assert!(err.to_string().contains("block 1"), "{}", err);
    }
}

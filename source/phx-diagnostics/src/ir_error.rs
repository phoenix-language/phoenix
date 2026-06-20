//! IR validation failure types (internal compiler invariant violations).
//!
//! These errors are **not** produced by invalid Phoenix source. They indicate that a lowered
//! [`IrFunction`](phx_compiler::ir::IrFunction) has a malformed control-flow graph or violates
//! operand-stack discipline before bytecode emission.
//!
//! ## User errors vs internal errors
//!
//! Type checking and name resolution catch source-level mistakes. IR validation runs after
//! [`lower_functions`](phx_compiler::lower::func::lower_functions) and before
//! [`codegen_module`](phx_compiler::codegen::codegen_module). A correct lowering pass should
//! always produce IR that passes validation; [`IrError`] variants mean the lowering driver or
//! stack-effect tables disagree with the emitted CFG. The driver reports them as internal compiler
//! errors (`E4002`) rather than actionable source diagnostics.
//!
//! ## Owning pass
//!
//! | Item | Producer |
//! | --- | --- |
//! | Structural [`IrError`] variants | [`validate_function`](phx_compiler::ir::validate::validate_function) — terminators, jump targets, loop-exit placeholders |
//! | Stack [`IrError`] variants | [`analyze_ir_stack_cfg`](phx_compiler::ir::stack_effect::analyze_ir_stack_cfg) — per-instruction depth simulation |
//! | [`IrBag`] | [`validate_ir`](phx_compiler::ir::validate::validate_ir) — aggregates per-function failures across a module |
//!
//! Validation is gated by [`validation_enabled`](phx_compiler::ir::validate::validation_enabled)
//! in release builds unless `PHX_VALIDATE_IR=1` is set.

use core::fmt;

use crate::LocatedError;
use crate::Span;
use crate::code::DiagnosticCode;

/// An IR validation error when lowered CFG or stack discipline is inconsistent.
///
/// Each variant documents a specific invariant checked between lowering and codegen. Valid Phoenix
/// programs that compiled successfully through type checking should never surface these errors to
/// the user.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IrError {
    /// Function has no basic blocks.
    ///
    /// Every lowered function must contain at least an entry block; an empty block list means
    /// lowering returned a shell [`IrFunction`] without emitting instructions.
    EmptyFunction {
        /// Lowered function definition index.
        def_index: u32,
    },
    /// Block ends without a terminator and has no fallthrough successor.
    ///
    /// Non-terminal blocks must end with a control-flow terminator or fall through to the next
    /// sequential block index. A block with trailing non-terminator instructions and no successor
    /// leaves the CFG incomplete.
    MissingTerminator {
        /// Lowered function definition index.
        def_index: u32,
        /// Basic block index.
        block: u32,
    },
    /// Instruction appears after a control-flow terminator.
    ///
    /// Terminators (`Return`, `Jump`, conditional branches, etc.) must be the last instruction
    /// in a basic block. Any following instruction is unreachable in the IR model and indicates
    /// a lowering emission bug.
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
    ///
    /// Branch and jump instructions must reference an existing block index in the same function.
    /// Out-of-range targets usually mean a stale block id or an off-by-one in the lowering driver.
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
    ///
    /// Break/continue lowering emits placeholder targets in the high address range; after the
    /// loop body is complete, [`LowerCtx::patch_loop_exit_targets`](phx_compiler::lower::ctx::LowerCtx::patch_loop_exit_targets)
    /// must rewrite them to real block indices. An unpatched placeholder survives into validation.
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
    ///
    /// Stack-effect analysis walks each block simulating pushes and pops. Underflow means an
    /// instruction consumed more operands than were available — typically a mismatch between
    /// [`IrInst`](phx_compiler::ir::IrInst) lowering and
    /// [`apply_ir_stack_effect_typed`](phx_compiler::ir::stack_effect::apply_ir_stack_effect_typed).
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
    ///
    /// Phoenix IR has no phi nodes; merge blocks require identical operand-stack depth on every
    /// incoming edge. Mismatch usually means `if`/`match` arms left different stack depths before
    /// joining.
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
    ///
    /// All IR validation failures map to `E4002` and render as internal compiler errors.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        DiagnosticCode::new("E4002")
    }

    /// Source span when available.
    ///
    /// Function-wide structural failures (empty CFG, missing terminator, invalid jump target)
    /// have no single instruction span. Instruction-level violations return the span attached
    /// during lowering.
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
///
/// Success means every function in the module passed structural and stack checks; failure is
/// always an [`IrBag`].
///
/// # Errors
///
/// The `Err` branch is an [`IrBag`] when validation recorded one or more [`IrError`] invariant
/// violations.
pub type IrResult<T> = Result<T, IrBag>;

/// Collected IR validation diagnostics.
///
/// [`validate_ir`](phx_compiler::ir::validate::validate_ir) records one [`LocatedError`] per
/// function failure, keyed by the owning module id from the typed program's definition table.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IrBag {
    errors: Vec<LocatedError<IrError>>,
}

impl IrBag {
    /// Creates an empty bag.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an error for `module`.
    ///
    /// `module` is the compilation-unit module index from
    /// [`ResolvedProgram`](phx_compiler::resolver::ResolvedProgram), not a source file path.
    ///
    /// # Panics
    ///
    /// Never panics.
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

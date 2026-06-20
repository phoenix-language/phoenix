//! Lowered function bodies in Phoenix IR.
//!
//! Each [`IrFunction`] is one monomorphized definition emitted by [`crate::lower::lower`]. Block 0
//! is the CFG entry; [`IrBasicBlock`] terminators encode all control flow. Codegen maps blocks to
//! bytecode basic blocks and uses [`local_count`](IrFunction::local_count) for frame layout.
//!
//! ## Invariants
//!
//! - [`IrFunction::params`] and [`IrFunction::return_type`] match the typed signature on
//!   [`IrFunction::def`].
//! - Local slot `0..params.len()` hold parameters; slots `params.len()..local_count` are
//!   temporaries allocated in [`TypedProgram::functions`](crate::typeck::TypedProgram::functions).
//! - [`validate_function`](super::validate_function) checks CFG shape and operand-stack depth at
//!   merge blocks before codegen when validation is enabled.

use super::block::IrBasicBlock;
use super::inst::IrFunctionId;
use crate::resolver::DefId;
use crate::typeck::TypeId;

/// One lowered function: signature, locals, and a CFG of [`IrBasicBlock`]s.
///
/// Identified within an [`IrModule`](super::IrModule) by [`IrFunction::id`] (dense index) and
/// within the resolver graph by [`IrFunction::def`]. Instruction payloads live in
/// [`IrBasicBlock::insts`] as [`SpannedInst`](super::SpannedInst) for diagnostic spans.
#[derive(Debug, Clone)]
pub struct IrFunction {
    /// Dense index in [`IrModule::functions`](super::IrModule::functions); stable for the module lifetime.
    pub id: IrFunctionId,
    /// Resolved definition (function, method, or closure body) this CFG implements.
    pub def: DefId,
    /// Parameter [`TypeId`]s in source order (arity matches the typed signature).
    pub params: Vec<TypeId>,
    /// Declared return [`TypeId`] (including `()` for void-like functions).
    pub return_type: TypeId,
    /// Total local slots: parameters plus temporaries (`local_count >= params.len()`).
    pub local_count: u32,
    /// CFG basic blocks; index `0` is the entry block.
    pub blocks: Vec<IrBasicBlock>,
}

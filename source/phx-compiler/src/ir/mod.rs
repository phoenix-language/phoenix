//! Phoenix intermediate representation (IR).
//!
//! IR is the handoff between type checking and bytecode codegen. It is built only from
//! [`TypedProgram`](crate::typeck::TypedProgram) — lowering must not re-walk unresolved AST.
//!
//! ## Invariants
//!
//! - Every value-producing instruction is associated with a [`TypeId`](crate::typeck::TypeId)
//!   or a typed local slot.
//! - Every emitted instruction carries a [`Span`](phx_diagnostics::Span) via [`SpannedInst`]
//!   for backend diagnostics. Spans are serialized to PHX0 section 5 (PC span map) in dev builds;
//!   see [`debug.md`](../../../docs/design/features/debug.md).
//! - Control flow is explicit in [`IrBasicBlock`] terminators.
//! - The entry function for executables is `main :: () => ()` (resolved before lowering).
//! - Local slot indices must come from [`TypedProgram::functions`](crate::typeck::TypedProgram::functions);
//!   lowering must not re-allocate slots from the AST.
//!
//! ### Stack discipline at merge blocks
//!
//! `if` and `match` expression lowering join branches at a merge block without phi nodes:
//! each branch must leave the same stack depth (typically one unified result value; `match`
//! stores the scrutinee in a temp local first). Short-circuit `&&` / `||` use dedicated CFG
//! (`lower_short_circuit_bool` in `lower/expr.rs`). [`validate_ir`](validate::validate_ir)
//! runs when [`validation_enabled`](validate::validation_enabled) is true (debug/test builds,
//! or `PHX_VALIDATE_IR=1`); it simulates stack depth and rejects join mismatches before codegen;
//! [`phx_bytecode::verify`](../../../phx-bytecode/src/verify.rs) enforces stack effects on emitted
//! bytecode.
//!
//! ### Ownership
//!
//! Moves and use-after-move are enforced in typeck only. IR uses [`IrInst::LoadLocal`] /
//! [`IrInst::StoreLocal`] without move flags. Post-MVP borrow checking may add explicit move/drop
//! instructions — see [`ownership.md`](../../../docs/design/features/ownership.md).

mod block;
mod const_lit;
mod func;
mod inst;
mod spanned;
mod stack_effect;
mod validate;

#[allow(unused_imports)]
pub(crate) use stack_effect::apply_ir_stack_effect_emit;
pub use stack_effect::{StackSimError, compute_ir_stack_max};

pub use block::IrBasicBlock;
pub use const_lit::IrConst;
pub use func::IrFunction;
pub use inst::{IrBinOp, IrFunctionId, IrInst, LocalSlot};
pub use spanned::SpannedInst;
pub use validate::{validate_function, validate_ir, validation_enabled};

use crate::resolver::DefId;

/// A compiled module in IR form (single-file MVP).
#[derive(Debug, Clone)]
pub struct IrModule {
    /// Functions lowered from the typed program.
    pub functions: Vec<IrFunction>,
    /// [`DefId`] of `main`, if present.
    pub entry: Option<DefId>,
    /// Module constant pool (indices used by [`IrInst::Const`]).
    pub constants: Vec<IrConst>,
}

impl IrModule {
    /// Creates an empty module (placeholder until lowering is implemented).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            functions: Vec::new(),
            entry: None,
            constants: Vec::new(),
        }
    }
}

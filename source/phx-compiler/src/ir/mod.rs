//! Phoenix intermediate representation (IR).
//!
//! IR is the handoff between type checking and bytecode codegen. It is built only from
//! [`TypedProgram`](crate::typeck::TypedProgram) — lowering must not re-walk unresolved AST.
//!
//! ## Invariants
//!
//! - Every value-producing instruction is associated with a [`TypeId`](crate::typeck::TypeId)
//!   or a typed local slot.
//! - Control flow is explicit in [`IrBasicBlock`] terminators.
//! - The entry function for executables is `main :: () => ()` (resolved before lowering).

mod block;
mod func;
mod inst;

pub use block::IrBasicBlock;
pub use func::IrFunction;
pub use inst::{IrBinOp, IrFunctionId, IrInst, LocalSlot};

use crate::resolver::DefId;

/// A compiled module in IR form (single-file MVP).
#[derive(Debug, Clone)]
pub struct IrModule {
    /// Functions lowered from the typed program.
    pub functions: Vec<IrFunction>,
    /// [`DefId`] of `main`, if present.
    pub entry: Option<DefId>,
}

impl IrModule {
    /// Creates an empty module (placeholder until lowering is implemented).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            functions: Vec::new(),
            entry: None,
        }
    }
}

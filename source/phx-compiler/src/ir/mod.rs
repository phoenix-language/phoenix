//! Phoenix intermediate representation (IR).
//!
//! IR is the handoff between type checking and bytecode codegen. It is built only from
//! [`TypedProgram`](crate::typeck::TypedProgram) — lowering must not re-walk unresolved AST.
//!
//! ## Public API
//!
//! | Type / function | Module | Role |
//! |---|---|---|
//! | [`IrModule`] | `mod` | Whole lowered unit (functions, entry, constants) |
//! | [`IrFunction`] | [`func`] | One CFG + signature |
//! | [`IrInst`], [`IrBinOp`], [`IrFunctionId`] | [`inst`] | Instructions and indices |
//! | [`IrBasicBlock`], [`SpannedInst`] | `block`, `spanned` | CFG nodes and span-tagged insts |
//! | [`IrConst`] | `const_lit` | Module constant pool entries |
//! | [`validate_ir`], [`validation_enabled`] | `validate` | Pre-codegen structural checks |
//! | [`compute_ir_stack_max`], [`StackSimError`] | `stack_effect` | Stack depth analysis |
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
///
/// Produced by [`crate::lower::lower`] from a [`TypedProgram`](crate::typeck::TypedProgram) and
/// consumed by [`crate::codegen::codegen_module`] (after optional [`validate_ir`]). This is the
/// last structured graph before bytecode emission; embedders should use
/// [`crate::facade::compile_to_module`] rather than holding an [`IrModule`] across releases.
///
/// ## Lookup
///
/// - Resolve a function body by [`IrFunctionId::index`] into [`Self::functions`].
/// - Match [`IrFunction::def`] against [`TypedProgram::functions`](crate::typeck::TypedProgram::functions)
///   for slot layout and expression metadata during validation.
/// - [`Self::entry`] mirrors the typed program entry and selects the VM root when present.
#[derive(Debug, Clone)]
pub struct IrModule {
    /// All lowered functions in dense [`IrFunctionId`] order (index `i` ↔ `IrFunctionId::from_raw(i)`).
    pub functions: Vec<IrFunction>,
    /// [`DefId`] of `main :: () => ()` when the compilation unit defines an entry point.
    pub entry: Option<DefId>,
    /// Module constant pool; [`IrInst::Const`] and [`IrInst::MakeStr`] reference indices here.
    pub constants: Vec<IrConst>,
}

impl IrModule {
    /// Returns an empty module with no functions, entry, or constants.
    ///
    /// Used by tests and incremental lowering scaffolding; production pipelines populate fields via
    /// [`crate::lower::lower`].
    #[must_use]
    pub fn empty() -> Self {
        Self {
            functions: Vec::new(),
            entry: None,
            constants: Vec::new(),
        }
    }
}

//! AST-to-IR lowering.
//!
//! Consumes a [`TypedProgram`](crate::typeck::TypedProgram) and produces an [`IrModule`](crate::ir::IrModule).
//! Does not read source text or build types — type checking must run first.
//!
//! ## Module map
//!
//! - [`func`] — top-level and impl function bodies
//! - [`expr`] — expression trees → instruction streams
//! - [`stmt`] — statements, bindings, control flow

mod expr;
mod func;
mod stmt;

use crate::ir::IrModule;
use crate::typeck::TypedProgram;

/// Lowers `typed` to IR.
///
/// MVP stub: returns an empty [`IrModule`]. Implementation will walk typed AST nodes,
/// reuse [`ExprId`](crate::typeck::ExprId) → [`TypeId`](crate::typeck::TypeId) from typeck,
/// and emit [`IrInst`](crate::ir::IrInst) into [`IrBasicBlock`](crate::ir::IrBasicBlock)s.
#[must_use]
pub fn lower(typed: &TypedProgram) -> IrModule {
    let _functions = func::lower_functions(typed);
    IrModule::empty()
}

//! AST-to-IR lowering.
#![allow(
    clippy::collapsible_if,
    clippy::match_same_arms,
    clippy::needless_pass_by_value,
    clippy::manual_let_else,
    unreachable_patterns
)] // `#[non_exhaustive]` AST enums need fallback `_` arms
//!
//! Consumes a [`TypedProgram`](crate::typeck::TypedProgram) and produces an [`IrModule`](crate::ir::IrModule).
//! Does not read source text or build types — type checking must run first.
//!
//! ## Module map
//!
//! - [`func`] — top-level and impl function bodies
//! - [`expr`] — expression trees → instruction streams
//! - [`stmt`] — statements, bindings, control flow

mod ctx;
mod expr;
mod func;
mod stmt;

use crate::ir::IrModule;
use crate::typeck::TypedProgram;

/// Lowers `typed` to IR.
///
/// Walks typed AST nodes in the same expression order as typeck, uses
/// [`TypedProgram::functions`](crate::typeck::TypedProgram::functions) for [`LocalSlot`](crate::typeck::LocalSlot)
/// indices, and reads expression types from [`TypedProgram::expr_types`](crate::typeck::TypedProgram::expr_types).
#[must_use]
pub fn lower(typed: &TypedProgram) -> IrModule {
    let mut constants = Vec::new();
    let functions = func::lower_functions(typed, &mut constants);
    IrModule {
        functions,
        entry: typed.entry,
        constants,
    }
}

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

use crate::ir::{IrFunction, IrModule};
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

/// Lowers only functions defined in `module_id` (slice of a full [`lower`] result).
#[must_use]
pub fn lower_module(typed: &TypedProgram, module_id: u32) -> IrModule {
    let full = lower(typed);
    let functions: Vec<IrFunction> = full
        .functions
        .iter()
        .filter(|f| {
            typed
                .resolved
                .defs
                .get(f.def.index() as usize)
                .is_some_and(|d| d.module == module_id)
        })
        .cloned()
        .collect();
    let entry = typed.entry.filter(|&main| {
        typed
            .resolved
            .defs
            .get(main.index() as usize)
            .is_some_and(|d| d.module == module_id)
    });
    IrModule {
        functions,
        entry,
        constants: full.constants,
    }
}

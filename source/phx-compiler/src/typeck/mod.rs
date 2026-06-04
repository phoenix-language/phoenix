//! Type checking for a resolved Phoenix program.
#![allow(
    clippy::collapsible_if,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::ref_option,
    clippy::trivially_copy_pass_by_ref
)] // `#[non_exhaustive]` AST enums need fallback `_` arms; MVP checker favors clarity
//!
//! Consumes [`ResolvedProgram`] and produces [`TypedProgram`] with interned types per expression.

mod bindings;
mod builtins;
mod check;
mod display;
mod layout;
mod lower_ty;
mod ops;
mod ownership;
mod primitive;
mod types;
mod unify;

pub use bindings::{Binding, BindingKind, FunctionLayout, LocalSlot};
pub use check::type_check;
pub use display::format_type;
pub use layout::{ProgramLayout, VariantKind};
pub use primitive::{primitive_kind_for_type, primitive_load_signed, slot_kind_for_binding};
pub use types::{ExprId, Ty, TypeId, TypeInterner};

use crate::resolver::ResolvedProgram;

/// Result of type-checking a [`ResolvedProgram`].
///
/// Carries the full resolved AST, type interner, and layout metadata for lowering. Field layout is
/// not stable for external consumers; prefer [`crate::compile_to_module`] for bytecode output.
#[derive(Debug, Clone)]
pub struct TypedProgram {
    /// Resolved input (AST + defs).
    pub resolved: ResolvedProgram,
    /// Interned types for the unit.
    pub types: TypeInterner,
    /// Expression types by [`ExprId`].
    pub expr_types: std::collections::HashMap<ExprId, TypeId>,
    /// Per-function local layouts for lowering.
    pub functions: Vec<bindings::FunctionLayout>,
    /// `main` definition id when present.
    pub entry: Option<crate::resolver::DefId>,
    /// Struct/enum layouts and bytecode type ids.
    pub layout: ProgramLayout,
}

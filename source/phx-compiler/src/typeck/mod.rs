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
mod bounds;
mod builtins;
mod check;
mod display;
mod infer;
mod layout;
mod lower_ty;
mod mangle;
mod mono;
mod ops;
mod ownership;
mod primitive;
mod std_kernel;
mod std_trait_kernel;
mod subst;
mod types;
mod unify;

#[allow(unused_imports)]
pub use mono::{
    CrossCrateMonoReq, MonoInst, TypeMonoInst, TypeMonoKind, apply_mono_worklist,
    collect_cross_crate_mono_reqs, is_generic_fn_template, monomorphize,
};

pub use bindings::{Binding, BindingKind, FunctionLayout, LocalSlot};
pub use check::type_check;
pub use display::format_type;
pub use layout::{EnumLayout, ProgramLayout, StructLayout, VariantKind};
pub use mangle::mangle_export_id;
pub use primitive::{primitive_kind_for_type, primitive_load_signed, slot_kind_for_binding};
pub use std_kernel::{StdKernel, TryFailureMode, TrySiteMeta};
pub use std_trait_kernel::StdTraitKernel;
pub use types::{ExprId, Ty, TypeId, TypeInterner};

use crate::resolver::{DefId, ResolvedProgram};

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
    /// Monomorphized `DefId` → generic template `DefId` for AST lookup during lowering.
    pub specialized_from: std::collections::HashMap<DefId, DefId>,
    /// Explicit generic function instantiations collected during type-checking.
    pub mono_insts: Vec<MonoInst>,
    /// Expanded monomorphized alias types keyed by `(template, args)`.
    pub specialized_aliases: std::collections::HashMap<layout::TypeMonoKey, TypeId>,
    /// Std `Option` / `Result` kernel for `?` sugar (empty when std is not linked).
    pub std_kernel: StdKernel,
    /// Std core trait definition ids (empty when std is not linked).
    pub std_trait_kernel: StdTraitKernel,
    /// `expr?` lowering metadata keyed by postfix expression id.
    pub try_sites: std::collections::HashMap<ExprId, TrySiteMeta>,
    /// Compiler builtin method sites on primitives (`eq`, `clone`).
    pub primitive_method_sites: std::collections::HashMap<ExprId, PrimitiveMethodSite>,
    /// Trait associated fn call sites (`Target::from`) → callee fn def.
    pub associated_fn_sites: std::collections::HashMap<ExprId, DefId>,
    /// Value types for defs (functions, types, consts) from the template pass; used when re-checking mono bodies.
    pub value_types: std::collections::HashMap<crate::resolver::DefId, TypeId>,
}

/// Lowering hint for trait method calls on primitive receivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveMethodSite {
    /// `PartialEq::eq` — emit `BinOp::Eq`.
    Eq,
    /// `Clone::clone` — identity (value already on stack).
    Clone,
}

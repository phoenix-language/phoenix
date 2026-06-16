//! Type checking for a resolved Phoenix program.
#![allow(
    clippy::collapsible_if,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::ref_option,
    clippy::trivially_copy_pass_by_ref
)]
//!
//! Consumes [`ResolvedProgram`] and produces [`TypedProgram`] with interned types per expression.

mod bindings;
mod bounds;
mod builtins;
mod check;
mod display;
mod infer;
mod intrinsic_kernel;
mod layout;
mod lower_ty;
mod mangle;
mod mono;
mod ops;
mod ownership;
mod primitive;
mod std_kernel;
mod subst;
mod trait_defaults;
mod type_size;
mod types;
mod unify;

pub(crate) use mono::{
    CrossCrateMonoReq, MonoInst, apply_mono_worklist, collect_cross_crate_mono_reqs,
    is_generic_fn_template, is_generic_impl_method_template,
};

pub use bindings::{Binding, BindingKind, ForInPlan, FunctionLayout, LocalSlot};
pub use check::type_check;
pub use display::format_type;
pub use intrinsic_kernel::IntrinsicSite;
pub use layout::{EnumLayout, ProgramLayout, StructLayout, VariantKind};
pub use mangle::mangle_export_id;
pub use primitive::{primitive_kind_for_type, primitive_load_signed, slot_kind_for_binding};
pub use std_kernel::{TryFailureMode, TrySiteMeta};
pub use trait_defaults::lookup_function;
pub use type_size::type_byte_size;
pub use types::{ExprId, Ty, TypeId, TypeInterner};

use crate::lang_items::LangItemRegistry;
use crate::resolver::{DefId, ResolvedProgram};
use phx_diagnostics::Span;

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
    ///
    /// Typeck assigns ids in pre-order AST visit order (`check_expr_node` / `alloc_expr_id`).
    /// Each function records its half-open id range in [`bindings::FunctionLayout::expr_start`] /
    /// [`bindings::FunctionLayout::expr_end`]. Lowering must consume exactly that range in the
    /// same visit order; a missing entry or cursor mismatch is an internal compiler error.
    pub expr_types: std::collections::HashMap<ExprId, TypeId>,
    /// Expression types keyed by `(module_id, source span)` for lint discard checks.
    pub expr_span_types: std::collections::HashMap<(u32, Span), TypeId>,
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
    /// Language item registry (`Option`, intrinsics, core traits).
    pub lang_items: LangItemRegistry,
    /// `expr?` lowering metadata keyed by postfix expression id.
    pub try_sites: std::collections::HashMap<ExprId, TrySiteMeta>,
    /// Indirect fn pointer call sites keyed by postfix `Call` expression id.
    pub indirect_call_sites: std::collections::HashMap<ExprId, IndirectCallMeta>,
    /// VM intrinsic call sites keyed by postfix `Call` expression id.
    pub intrinsic_call_sites: std::collections::HashMap<ExprId, IntrinsicSite>,
    /// Compile-time `size_of` results keyed by postfix `Call` expression id.
    pub size_of_literals: std::collections::HashMap<ExprId, u32>,
    /// Compiler builtin method sites on primitives (`eq`, `clone`).
    pub primitive_method_sites: std::collections::HashMap<ExprId, PrimitiveMethodSite>,
    /// Trait associated fn call sites (`Target::from`) → callee fn def.
    pub associated_fn_sites: std::collections::HashMap<ExprId, DefId>,
    /// Method call sites (`recv.method`) keyed by postfix expression id.
    ///
    /// Typeck records the resolved template and mono args; monomorphization patches `template`
    /// to the specialized callee. Lowering must consume this table and must not re-resolve methods.
    pub method_call_sites: std::collections::HashMap<ExprId, MethodCallSiteMeta>,
    /// Value types for defs (functions, types, consts) from the template pass; used when re-checking mono bodies.
    pub value_types: std::collections::HashMap<crate::resolver::DefId, TypeId>,
    /// Trait default methods synthesized for empty/partial impl blocks.
    pub inherited_trait_methods: trait_defaults::InheritedTraitMethods,
    /// Functions that require `unsafe` at call sites (top-level `unsafe fn`, `unsafe trait` methods, etc.).
    pub fn_effective_unsafe: std::collections::HashMap<crate::resolver::DefId, bool>,
}

/// Lowering hint for indirect function pointer calls (`CallIndirect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndirectCallMeta {
    /// Bytecode type-table `FnSig` id for verifier arity contract.
    pub sig_type_id: u32,
    /// Callee parameter count.
    pub expected_arity: u32,
    /// `true` when the call target is an `extern "C"` symbol.
    pub foreign: bool,
}

/// Resolved method call metadata keyed by postfix expression [`ExprId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodCallSiteMeta {
    /// Generic template or monomorphized callee `DefId` after specialization.
    pub template: DefId,
    /// Type arguments for monomorphization (impl + method generics), in parameter order.
    pub mono_args: Vec<TypeId>,
}

/// Lowering hint for trait method calls on primitive receivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveMethodSite {
    /// `PartialEq::eq` — emit `BinOp::Eq`.
    Eq,
    /// `Clone::clone` — identity (value already on stack).
    Clone,
}

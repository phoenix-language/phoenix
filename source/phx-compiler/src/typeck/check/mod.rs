//! Type-checking driver and AST walk.
//!
//! Owns [`TypeChecker`], which walks the resolved AST in two phases: collect top-level types and
//! signatures, then check function and impl bodies. Fills [`TypeInterner`], per-expression
//! [`TypeId`] maps, layout tables, and lowering site metadata. Errors accumulate in
//! [`TypeCheckBag`] without stopping at the first failure.
//!
//! On success, [`type_check`] merges inherited trait defaults and runs
//! [`super::mono::monomorphize`] on explicit generic instantiation sites.
//!
//! ## Module map
//!
//! - [`ctx`] — constructors, scope, errors, finish
//! - [`decl`] — top-level declaration collection
//! - [`impls`] — inherent and trait impl checking
//! - [`stmt`] — statements, blocks, loops, drops
//! - [`expr`] — expressions, postfix, calls
//! - [`pattern`] — match arms and patterns
//! - [`intrinsic`] — VM intrinsics and extern calls

mod ctx;
mod decl;
mod expr;
mod impls;
mod intrinsic;
mod pattern;
mod stmt;

use std::collections::HashMap;

use phx_diagnostics::{Span, TypeCheckBag};
use phx_syntax::Symbol;

use crate::lang_items::LangItemRegistry;
use crate::resolver::{DefId, ResolvedProgram};
use crate::typeck::IndirectCallMeta;
use crate::typeck::MethodCallSiteMeta;
use crate::typeck::PrimitiveMethodSite;
use crate::typeck::bindings::{FunctionLayout, FunctionLayoutBuilder};
use crate::typeck::intrinsic_kernel::IntrinsicSite;
use crate::typeck::layout::{ProgramLayout, TypeMonoKey};
use crate::typeck::lower_ty::TypeDefMap;
use crate::typeck::mono::{MonoInst, TypeMonoInst};
use crate::typeck::ownership::OwnershipTracker;
use crate::typeck::std_kernel::TrySiteMeta;
use crate::typeck::subst::Substitution;
use crate::typeck::trait_defaults;
use crate::typeck::types::{ExprId, TypeId, TypeInterner};

/// Collected struct field types.
#[derive(Debug, Clone)]
pub struct StructFields {
    /// Field name → type.
    pub fields: HashMap<Symbol, TypeId>,
}

/// Type checker state for one compilation unit.
///
/// Walks the resolved AST, fills [`TypeInterner`] and per-expression [`TypeId`]s, and records
/// errors in a [`TypeCheckBag`] without stopping at the first failure.
pub struct TypeChecker<'a> {
    resolved: &'a ResolvedProgram,
    types: TypeInterner,
    bag: TypeCheckBag,
    expr_types: HashMap<ExprId, TypeId>,
    /// Expression types keyed by `(module_id, source span)` for lint discard checks.
    expr_span_types: HashMap<(u32, Span), TypeId>,
    next_expr: u32,
    type_defs: TypeDefMap,
    value_types: HashMap<DefId, TypeId>,
    struct_fields: HashMap<DefId, StructFields>,
    fn_ret: Option<TypeId>,
    ownership: OwnershipTracker,
    unit: TypeId,
    bool_ty: TypeId,
    functions: Vec<FunctionLayout>,
    layout: Option<FunctionLayoutBuilder>,
    ctor_expected: Option<TypeId>,
    /// Nesting depth of `while` / `loop` bodies being checked.
    loop_depth: u32,
    /// Layout scope depth at each active loop body entry (before `check_block` `enter_scope`).
    loop_body_scope_depths: Vec<u32>,
    /// Struct/enum layouts for lowering and codegen.
    program_layout: ProgramLayout,
    next_type_id: u32,
    /// When checking inherent impl members, the receiver type (`Self`).
    impl_self_type: Option<TypeId>,
    /// When checking a trait impl, `(implementer type def, trait def)`.
    active_trait_impl: Option<(DefId, DefId)>,
    /// Associated types assigned on the active trait impl (`Item` → concrete type).
    impl_assoc_types: HashMap<Symbol, TypeId>,
    /// Abstract associated types while collecting a trait definition.
    trait_assoc_abstract: HashMap<Symbol, TypeId>,
    /// Module being collected or checked.
    current_module: u32,
    /// Active type substitution when checking a monomorphized clone.
    subst: Option<Substitution>,
    /// Template definition when checking a monomorphized function clone.
    mono_template_def: Option<DefId>,
    /// Explicit generic instantiations to specialize after the main pass.
    mono_insts: Vec<MonoInst>,
    /// Explicit generic type instantiations to specialize after the main pass.
    type_mono_insts: Vec<TypeMonoInst>,
    /// Expanded alias types keyed by monomorphization key (filled during checking).
    specialized_aliases: HashMap<TypeMonoKey, TypeId>,
    /// Compiler-known std definitions (intrinsics, `Option`/`Result`, traits).
    lang_items: LangItemRegistry,
    /// `expr?` sites for lowering.
    try_sites: HashMap<ExprId, TrySiteMeta>,
    /// Primitive trait method sites for lowering.
    primitive_method_sites: HashMap<ExprId, PrimitiveMethodSite>,
    /// Trait associated fn call sites (`Target::from`) → monomorphized or template fn def.
    associated_fn_sites: HashMap<ExprId, DefId>,
    /// Method call sites (`recv.method`) → resolved callee and mono args.
    method_call_sites: HashMap<ExprId, MethodCallSiteMeta>,
    /// Indirect fn pointer call sites for lowering.
    indirect_call_sites: HashMap<ExprId, IndirectCallMeta>,
    /// VM intrinsic call sites (`alloc_bytes`, …).
    intrinsic_call_sites: HashMap<ExprId, IntrinsicSite>,
    /// Compile-time `size_of` results keyed by postfix `Call` expression id.
    size_of_literals: HashMap<ExprId, u32>,
    /// Nesting depth of `unsafe` blocks and `unsafe fn` bodies.
    unsafe_depth: u32,
    /// Pending synthetic fn defs for inherited trait defaults (merged into resolved at finish).
    pending_inherited_defs: Vec<crate::resolver::Def>,
    /// Inherited trait default bodies keyed by synthetic `DefId`.
    inherited_trait_methods: trait_defaults::InheritedTraitMethods,
    /// Inherited methods grouped by trait impl key (for body type checking).
    inherited_by_inst: trait_defaults::InheritedByInst,
    /// Trait defs marked `unsafe trait`.
    trait_unsafe: HashMap<DefId, bool>,
    /// Fn defs that require `unsafe` at call sites.
    fn_effective_unsafe: HashMap<DefId, bool>,
}

/// Maximum tuple-struct field index pre-interned for layout collection (`"0"` …).
const TUPLE_FIELD_SYMBOL_LIMIT: usize = 64;

fn seed_tuple_field_symbols(interner: &mut phx_syntax::Interner) {
    for i in 0..TUPLE_FIELD_SYMBOL_LIMIT {
        let _ = interner.intern(&i.to_string());
    }
}

/// Runs type checking on `resolved`.
///
/// Public entry for the type-check pass. Seeds tuple field symbols, walks all modules via
/// [`TypeChecker`], merges synthesized trait default definitions into `resolved`, then
/// monomorphizes collected generic instantiation sites. The returned [`TypedProgram`] carries
/// expr types, layouts, and lowering metadata consumed by [`crate::lower::lower`].
///
/// # Errors
///
/// Returns [`TypeCheckBag`] when one or more type errors were collected during checking or
/// monomorphization.
pub fn type_check(mut resolved: ResolvedProgram) -> Result<super::TypedProgram, TypeCheckBag> {
    seed_tuple_field_symbols(&mut resolved.interner);
    let mut checker = TypeChecker::new(&resolved);
    checker.check_program();
    let mono_insts = checker.take_mono_insts();
    let type_mono_insts = checker.take_type_mono_insts();
    let (
        types,
        expr_types,
        expr_span_types,
        bag,
        functions,
        layout,
        specialized_aliases,
        lang_items,
        try_sites,
        primitive_method_sites,
        associated_fn_sites,
        method_call_sites,
        value_types,
        indirect_call_sites,
        intrinsic_call_sites,
        size_of_literals,
        pending_inherited_defs,
        inherited_trait_methods,
        fn_effective_unsafe,
    ) = checker.finish();
    if bag.has_errors() {
        return Err(bag);
    }
    trait_defaults::merge_pending_inherited_defs(&mut resolved, pending_inherited_defs);
    let entry = resolved.main_fn;
    let mut program = super::TypedProgram {
        resolved,
        types,
        expr_types,
        expr_span_types,
        functions,
        entry,
        layout,
        specialized_from: HashMap::new(),
        mono_insts: Vec::new(),
        specialized_aliases,
        lang_items,
        try_sites,
        primitive_method_sites,
        associated_fn_sites,
        method_call_sites,
        value_types,
        indirect_call_sites,
        intrinsic_call_sites,
        size_of_literals,
        inherited_trait_methods,
        fn_effective_unsafe,
    };
    let mono_bag = super::mono::monomorphize(&mut program, &mono_insts, &type_mono_insts);
    if mono_bag.has_errors() {
        return Err(mono_bag);
    }
    program.mono_insts = mono_insts;
    Ok(program)
}

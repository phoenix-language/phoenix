//! Trait default method inheritance for empty or partial trait impl blocks.

use std::collections::HashMap;

use phx_syntax::Symbol;
use phx_syntax::ast::decl::{Function, FunctionSig, TopLevelDecl};
use phx_syntax::ast::decl::{ImplMember, TraitItem};

use crate::resolver::{Def, DefId, DefIdOverflow, DefKind, ResolvedProgram};
use crate::typeck::TypedProgram;
use crate::typeck::layout::TraitInstKey;

/// Trait default methods synthesized for empty/partial impl blocks (`DefId` → body AST).
pub type InheritedTraitMethods = HashMap<DefId, Function>;

/// Inherited methods keyed by trait instantiation (for impl-body type checking).
pub type InheritedByInst = HashMap<TraitInstKey, Vec<(DefId, Function)>>;

/// Builds a [`Function`] from a trait method signature that carries a default body.
#[must_use]
pub fn sig_to_function(sig: &FunctionSig, trait_unsafe: bool) -> Option<Function> {
    let body = sig.body.clone()?;
    Some(Function {
        attrs: Vec::new(),
        derives: Vec::new(),
        directives: Vec::new(),
        unsafe_: trait_unsafe || sig.unsafe_,
        name: sig.name,
        generics: sig.generics.clone(),
        params: sig.params.clone(),
        ret: sig.ret.clone(),
        body,
    })
}

/// Allocates a synthetic `DefKind::Fn` id for an inherited trait method (not yet in `resolved.defs`).
///
/// # Errors
///
/// Returns [`DefIdOverflow`] when the definition table would exceed `u32::MAX`.
pub fn try_alloc_pending_inherited_fn_def(
    resolved: &ResolvedProgram,
    pending_count: usize,
) -> Result<DefId, DefIdOverflow> {
    let index = resolved
        .defs
        .len()
        .checked_add(pending_count)
        .ok_or(DefIdOverflow)?;
    DefId::try_from_index(index)
}

/// Pushes pending inherited fn defs into `resolved.defs`.
pub fn merge_pending_inherited_defs(resolved: &mut ResolvedProgram, pending: Vec<Def>) {
    resolved.defs.extend(pending);
}

/// Looks up a function body from the crate AST or inherited trait defaults.
#[must_use]
pub fn lookup_function(typed: &TypedProgram, def: DefId) -> Option<&Function> {
    find_function_in_ast(typed, def).or_else(|| typed.inherited_trait_methods.get(&def))
}

fn find_function_in_ast(typed: &TypedProgram, def: DefId) -> Option<&Function> {
    let lookup = typed.specialized_from.get(&def).copied().unwrap_or(def);
    let def_record = typed.resolved.defs.get(lookup.index() as usize)?;
    let module = typed
        .resolved
        .modules
        .iter()
        .find(|m| m.id == def_record.module)?;
    for item in &module.program.items {
        match &item.inner.decl {
            TopLevelDecl::Function(f) if def_matches(def_record, f) => return Some(f),
            TopLevelDecl::Impl { members, .. } => {
                for m in members {
                    if let ImplMember::Method(f) = m {
                        if def_matches(def_record, f) {
                            return Some(f);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn def_matches(def_record: &Def, f: &Function) -> bool {
    def_record.name == f.name.symbol && def_record.kind == DefKind::Fn
}

/// Inputs for synthesizing inherited trait default methods on an impl block.
pub struct InheritedSynthesisCtx<'a> {
    /// Resolved program (for pending def id allocation).
    pub resolved: &'a ResolvedProgram,
    /// Trait items from the trait definition.
    pub trait_items: &'a [TraitItem],
    /// Method names already written in the impl block.
    pub impl_method_names: &'a std::collections::HashSet<Symbol>,
    /// Module id owning the impl.
    pub module: u32,
    /// Whether the trait being implemented is `unsafe trait`.
    pub trait_unsafe: bool,
    /// Pending synthetic fn defs (merged into resolved at typeck finish).
    pub pending_inherited_defs: &'a mut Vec<Def>,
    /// Inherited bodies keyed by synthetic `DefId`.
    pub inherited_trait_methods: &'a mut InheritedTraitMethods,
    /// Trait instantiation key for this impl.
    pub inst_key: &'a TraitInstKey,
    /// Inherited methods grouped by trait impl key.
    pub inherited_by_inst: &'a mut InheritedByInst,
}

/// Synthesizes inherited trait default methods missing from an impl block.
///
/// # Errors
///
/// Returns [`DefIdOverflow`] when the definition table would exceed `u32::MAX`.
pub fn synthesize_inherited_methods(
    ctx: &mut InheritedSynthesisCtx<'_>,
) -> Result<Vec<(Symbol, DefId)>, DefIdOverflow> {
    let mut registered = Vec::new();
    for item in ctx.trait_items {
        let TraitItem::Method(sig) = item else {
            continue;
        };
        if sig.body.is_none() || ctx.impl_method_names.contains(&sig.name.symbol) {
            continue;
        }
        let Some(function) = sig_to_function(sig, ctx.trait_unsafe) else {
            continue;
        };
        let def =
            try_alloc_pending_inherited_fn_def(ctx.resolved, ctx.pending_inherited_defs.len())?;
        ctx.pending_inherited_defs.push(Def::new(
            DefKind::Fn,
            sig.name.symbol,
            sig.name.span,
            ctx.module,
            false,
            0,
        ));
        ctx.inherited_trait_methods.insert(def, function.clone());
        ctx.inherited_by_inst
            .entry(ctx.inst_key.clone())
            .or_default()
            .push((def, function));
        registered.push((sig.name.symbol, def));
    }
    Ok(registered)
}

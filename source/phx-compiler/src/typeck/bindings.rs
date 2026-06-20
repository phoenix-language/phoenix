//! Per-function local slots and binding metadata for lowering.
//!
//! While [`super::check`] walks a function body, a [`FunctionLayoutBuilder`] assigns stable
//! [`LocalSlot`] indices to parameters, `const`/`var` bindings, and compiler temps. The finished
//! [`FunctionLayout`] is stored on [`TypedProgram`](crate::typeck::TypedProgram) and consumed
//! verbatim by [`crate::lower::lower`] — slot order, drop plans, and `for`-loop desugaring must
//! match what type checking recorded.
//!
//! # Role in type checking
//!
//! This module does not assign expression types. It answers layout questions for a single
//! function: which slot holds a name (including shadowing), where match scrutinees land, which
//! [`DropEvent`]s fire at scope exit, and how each `for binding in iter` maps to iterator
//! protocol calls. [`super::ownership`] and drop planning in the check walk call into the
//! builder; lowering reads the finished layout only.
//!
//! # Local slots
//!
//! Slots are dense `0..N-1` indices: parameters are allocated first in source order, then body
//! locals and temps in visit order. Block exit does **not** reclaim slots — indices stay stable
//! for the whole function so bytecode can reference them by number. Shadowing is resolved by
//! scanning bindings newest-first ([`FunctionLayout::binding`]).
//!
//! # Side tables on [`FunctionLayout`]
//!
//! - [`FunctionLayout::match_temp_slots`] — anonymous scrutinee temps for `match`, in source order.
//! - [`FunctionLayout::drop_events`] — planned `Drop::drop` calls; lowering emits in reverse slot
//!   order per scope depth (see [`crate::lower::drop_glue`]).
//! - [`FunctionLayout::for_in_plans`] — iterator-protocol metadata for `for` loops; lowering
//!   desugars to `into_iter` / `next` / `Option` matching without re-resolving traits.
//! - [`FunctionLayout::expr_start`] / [`FunctionLayout::expr_end`] — [`super::ExprId`] range for
//!   expressions typed inside this function; used to index per-expression side tables on
//!   [`TypedProgram`].
//!
//! # Invariants
//!
//! Every [`Binding::slot`] is unique within a layout. [`FunctionLayoutBuilder::plan_drop`] records
//! at most one drop per slot. Match and `for` temps use scratch symbols from
//! [`phx_syntax::scratch_binding_symbol`] so they never collide with user names.

use phx_diagnostics::Span;
use phx_syntax::Symbol;

use super::types::TypeId;
use crate::resolver::DefId;

/// Lowering metadata for one `for binding in iter` loop (iterator protocol desugaring).
///
/// Type checking resolves `IntoIter`, `into_iter`, and `next` once and stores the concrete
/// types and [`DefId`]s here. Lowering emits a loop that calls `into_iter`, repeatedly invokes
/// `next`, matches on `Option`, and binds `binding` from the `Some` payload — it does not
/// re-run trait lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForInPlan {
    /// Loop binding name (`binding` in `for binding in …`).
    pub binding: Symbol,
    /// `IntoIter::Item` / loop binding type.
    pub item_ty: TypeId,
    /// Local holding iterator state after `into_iter`.
    pub iter_temp_slot: LocalSlot,
    /// Concrete `IntoIter::IntoIter` type.
    pub iter_state_ty: TypeId,
    /// Resolved `into_iter` function.
    pub into_iter_fn: DefId,
    /// Resolved `next` function on the iterator state type.
    pub next_fn: DefId,
    /// `Option<Item>` scrutinee type for each `next` call.
    pub option_ty: TypeId,
    /// Temp slot holding the latest `next()` result.
    pub option_match_temp: LocalSlot,
    /// `Some` variant name on `option_ty` (for pattern tests).
    pub some_variant: Symbol,
    /// Source span of the `for` statement.
    pub stmt_span: Span,
}

/// One compiler-planned `Drop::drop` call at a scope exit.
///
/// Recorded when a non-Copyable binding goes out of scope. Lowering walks
/// [`FunctionLayout::drop_events`] at block boundaries and emits drop glue in reverse slot order
/// within each depth so destructors run inner-to-outer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropEvent {
    /// Block depth of the binding being dropped.
    pub scope_depth: u32,
    /// Local slot holding the owned value.
    pub slot: LocalSlot,
    /// Type of the local.
    pub ty: TypeId,
    /// Resolved `Drop::drop` function definition.
    pub drop_fn: DefId,
    /// Wire [`phx_bytecode::PrimitiveKind`] when the local is scalar.
    pub prim_kind: u8,
    /// Source span of the binding being dropped.
    pub span: Span,
}

/// Dense local slot index within a function (parameters + locals + temps).
///
/// Slots are assigned sequentially by [`FunctionLayoutBuilder::alloc`] and never reused in the
/// same function. Lowering and bytecode refer to locals by this index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalSlot(u32);

impl LocalSlot {
    /// Creates a slot from a raw index.
    ///
    /// Prefer obtaining slots through [`FunctionLayoutBuilder::alloc`] so indices stay consistent
    /// with the binding table.
    #[must_use]
    pub const fn from_raw(index: u32) -> Self {
        Self(index)
    }

    /// Returns the raw index used in bytecode local operands.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Kind of binding occupying a local slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    /// Function parameter (allocated before body locals, in parameter list order).
    Param,
    /// `const` binding — value is immutable; may carry [`Binding::utf8_rodata`] for string literals.
    Const,
    /// `var` binding — mutable local.
    Var,
    /// Anonymous slot holding a `match` scrutinee (not a source name; uses a scratch symbol).
    MatchTemp,
}

/// One local binding with slot, type, and scope metadata.
///
/// Stored in slot-allocation order inside [`FunctionLayout::bindings`]. Name lookup for codegen
/// uses the innermost matching symbol ([`FunctionLayout::binding`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// Interned name (or scratch symbol for match temps).
    pub symbol: Symbol,
    /// Slot index assigned at introduction.
    pub slot: LocalSlot,
    /// Checked type of the binding.
    pub ty: TypeId,
    /// Parameter vs local kind.
    pub kind: BindingKind,
    /// Block nesting depth where this binding was introduced.
    pub scope_depth: u32,
    /// When `kind` is [`BindingKind::Const`] and the initializer was a UTF-8 `b"…"` literal,
    /// holds those bytes for compile-time `arr as str` lowering to rodata.
    pub utf8_rodata: Option<Vec<u8>>,
    /// Source span where the binding was introduced.
    pub declare_span: Span,
}

/// Layout of locals and lowering side tables for one function.
///
/// Produced at the end of type-checking a function body and keyed by [`DefId`] on
/// [`TypedProgram::functions`](crate::typeck::TypedProgram). Lowering must not invent
/// slots or drop calls beyond what this struct records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionLayout {
    /// Resolved function definition.
    pub def: DefId,
    /// Declared return type.
    pub return_type: TypeId,
    /// All bindings in slot order (parameters first, then body locals and temps).
    pub bindings: Vec<Binding>,
    /// Scrutinee temp slots for `match`, in source visit order.
    pub match_temp_slots: Vec<LocalSlot>,
    /// First [`super::ExprId`] raw index assigned while checking this function body.
    pub expr_start: u32,
    /// One past the last expression id for this function (start index of the next function, or
    /// total expression count for the last function).
    pub expr_end: u32,
    /// Planned scope-exit drop calls (lowering emits in reverse slot order per depth).
    pub drop_events: Vec<DropEvent>,
    /// `for`-loop desugaring plans in source visit order.
    pub for_in_plans: Vec<ForInPlan>,
}

impl FunctionLayout {
    /// Number of local slots (parameters + body locals + temps).
    ///
    /// Equal to [`FunctionLayout::bindings`] length when every slot has exactly one binding.
    #[must_use]
    pub fn local_count(&self) -> u32 {
        u32::try_from(self.bindings.len()).unwrap_or(u32::MAX)
    }

    /// Looks up the innermost binding for `symbol` (shadowing-safe).
    ///
    /// Returns `None` when the name is not in scope in this function's layout.
    #[must_use]
    pub fn binding(&self, symbol: Symbol) -> Option<&Binding> {
        self.bindings.iter().rfind(|b| b.symbol == symbol)
    }
}

/// Incrementally builds [`FunctionLayout`] while type-checking a function body.
///
/// Created at function entry, mutated throughout the check walk, and finalized with
/// [`FunctionLayoutBuilder::finish`]. Scope enter/exit only adjust depth counters — they do not
/// remove bindings from the vector.
#[derive(Debug)]
pub struct FunctionLayoutBuilder {
    def: DefId,
    return_type: TypeId,
    bindings: Vec<Binding>,
    next_slot: u32,
    match_temp_slots: Vec<LocalSlot>,
    match_temp_serial: u32,
    expr_start: u32,
    expr_end: u32,
    scope_depth: u32,
    drop_events: Vec<DropEvent>,
    planned_drop_slots: std::collections::HashSet<LocalSlot>,
    for_in_plans: Vec<ForInPlan>,
    for_in_serial: u32,
}

impl FunctionLayoutBuilder {
    /// Starts layout for `def` with `return_type`.
    ///
    /// Parameters should be allocated with [`FunctionLayoutBuilder::alloc`] before checking the
    /// body so their slots precede locals.
    #[must_use]
    pub fn new(def: DefId, return_type: TypeId) -> Self {
        Self {
            def,
            return_type,
            bindings: Vec::new(),
            next_slot: 0,
            match_temp_slots: Vec::new(),
            match_temp_serial: 0,
            expr_start: 0,
            expr_end: 0,
            scope_depth: 0,
            drop_events: Vec::new(),
            planned_drop_slots: std::collections::HashSet::new(),
            for_in_plans: Vec::new(),
            for_in_serial: 0,
        }
    }

    /// Current block nesting depth for drop planning.
    #[must_use]
    pub fn scope_depth(&self) -> u32 {
        self.scope_depth
    }

    /// Function definition being laid out.
    #[must_use]
    pub fn def(&self) -> DefId {
        self.def
    }

    /// Enters a nested block scope.
    ///
    /// New bindings record the incremented depth on [`Binding::scope_depth`]; slots are not
    /// reclaimed on exit.
    pub fn enter_scope(&mut self) {
        self.scope_depth = self.scope_depth.saturating_add(1);
    }

    /// Leaves a block scope.
    ///
    /// Bindings remain in the vector for stable [`LocalSlot`] indices; use
    /// [`FunctionLayoutBuilder::binding`] with shadowing semantics for name lookup.
    pub fn exit_scope(&mut self) {
        self.scope_depth = self.scope_depth.saturating_sub(1);
    }

    /// Records the expression id range checked for this function body.
    ///
    /// Set once after the expression walk completes so [`FunctionLayout::expr_start`] and
    /// [`FunctionLayout::expr_end`] bracket per-expression metadata on [`TypedProgram`].
    pub fn set_expr_range(&mut self, start: u32, end: u32) {
        self.expr_start = start;
        self.expr_end = end;
    }

    /// Allocates a slot to hold one `match` scrutinee for lowering.
    ///
    /// Registers the slot in [`FunctionLayout::match_temp_slots`] in allocation order.
    #[must_use]
    pub fn alloc_match_scrutinee_temp(&mut self, ty: TypeId, declare_span: Span) -> LocalSlot {
        let serial = self.match_temp_serial;
        self.match_temp_serial += 1;
        let symbol = phx_syntax::scratch_binding_symbol(serial);
        let slot = self.alloc(symbol, ty, BindingKind::MatchTemp, None, declare_span);
        self.match_temp_slots.push(slot);
        slot
    }

    /// Looks up the innermost binding for `symbol` (shadowing-safe).
    #[must_use]
    pub fn binding(&self, symbol: Symbol) -> Option<&Binding> {
        self.bindings.iter().rfind(|b| b.symbol == symbol)
    }

    /// Returns bindings introduced at `scope_depth` (stable slot order).
    ///
    /// Used when lowering must emit drops or debug info for all locals in a block.
    #[must_use]
    pub fn bindings_at_depth(&self, scope_depth: u32) -> Vec<&Binding> {
        self.bindings
            .iter()
            .filter(|b| b.scope_depth == scope_depth)
            .collect()
    }

    /// Allocates the next slot and records `symbol` with `ty` and `kind`.
    ///
    /// Returns the assigned [`LocalSlot`]; the binding is appended to the internal vector.
    #[must_use]
    pub fn alloc(
        &mut self,
        symbol: Symbol,
        ty: TypeId,
        kind: BindingKind,
        utf8_rodata: Option<Vec<u8>>,
        declare_span: Span,
    ) -> LocalSlot {
        let slot = LocalSlot::from_raw(self.next_slot);
        self.next_slot += 1;
        self.bindings.push(Binding {
            symbol,
            slot,
            ty,
            kind,
            scope_depth: self.scope_depth,
            utf8_rodata,
            declare_span,
        });
        slot
    }

    /// Records a planned drop if `slot` is not already scheduled.
    ///
    /// Duplicate drops for the same slot are ignored so recursive planning stays idempotent.
    pub fn plan_drop(&mut self, event: DropEvent) {
        if self.planned_drop_slots.insert(event.slot) {
            self.drop_events.push(event);
        }
    }

    /// Records iterator-protocol metadata for one `for` loop.
    pub fn plan_for_in(&mut self, plan: ForInPlan) {
        self.for_in_plans.push(plan);
    }

    /// Returns the next `for`-loop plan index and advances the serial counter.
    ///
    /// Used to correlate lowering sites with [`FunctionLayout::for_in_plans`] entries.
    #[must_use]
    pub fn next_for_in_plan_index(&mut self) -> u32 {
        let index = self.for_in_serial;
        self.for_in_serial += 1;
        index
    }

    /// Finishes the layout and returns an immutable [`FunctionLayout`].
    #[must_use]
    pub fn finish(self) -> FunctionLayout {
        FunctionLayout {
            def: self.def,
            return_type: self.return_type,
            bindings: self.bindings,
            match_temp_slots: self.match_temp_slots,
            expr_start: self.expr_start,
            expr_end: self.expr_end,
            drop_events: self.drop_events,
            for_in_plans: self.for_in_plans,
        }
    }
}

//! Per-function local slots and bindings for lowering.

use phx_syntax::Symbol;

use super::types::TypeId;
use crate::resolver::DefId;

/// Dense local slot index within a function (parameters + locals).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalSlot(u32);

impl LocalSlot {
    /// Creates a slot from a raw index.
    #[must_use]
    pub const fn from_raw(index: u32) -> Self {
        Self(index)
    }

    /// Returns the raw index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Kind of binding occupying a local slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    /// Function parameter.
    Param,
    /// `const` binding.
    Const,
    /// `var` binding.
    Var,
    /// Anonymous slot holding a `match` scrutinee (not a source name).
    MatchTemp,
}

/// One local binding with slot and type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// Interned name.
    pub symbol: Symbol,
    /// Slot index.
    pub slot: LocalSlot,
    /// Type of the binding.
    pub ty: TypeId,
    /// Parameter vs local kind.
    pub kind: BindingKind,
    /// Block nesting depth where this binding was introduced.
    pub scope_depth: u32,
}

/// Layout of locals for one function (for IR/codegen).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionLayout {
    /// Resolved function definition.
    pub def: DefId,
    /// Declared return type.
    pub return_type: TypeId,
    /// All bindings in slot order.
    pub bindings: Vec<Binding>,
    /// Scrutinee temp slots for `match`, in source visit order.
    pub match_temp_slots: Vec<LocalSlot>,
    /// First [`super::ExprId`] raw index assigned while checking this function body.
    pub expr_start: u32,
    /// One past the last expression id for this function (`expr_start` of the next function).
    pub expr_end: u32,
}

impl FunctionLayout {
    /// Number of local slots (parameters + body locals).
    #[must_use]
    pub fn local_count(&self) -> u32 {
        u32::try_from(self.bindings.len()).unwrap_or(u32::MAX)
    }

    /// Looks up the innermost binding for `symbol` (shadowing-safe).
    #[must_use]
    pub fn binding(&self, symbol: Symbol) -> Option<&Binding> {
        self.bindings.iter().rfind(|b| b.symbol == symbol)
    }
}

/// Builds [`FunctionLayout`] while type-checking a function body.
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
}

impl FunctionLayoutBuilder {
    /// Starts layout for `def` with `return_type`.
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
        }
    }

    /// Enters a nested block scope (recorded on each [`Binding::scope_depth`]; slots are not reclaimed).
    pub fn enter_scope(&mut self) {
        self.scope_depth = self.scope_depth.saturating_add(1);
    }

    /// Leaves a block scope (bindings remain for stable [`LocalSlot`] indices; use [`FunctionLayout::binding`] with shadowing).
    pub fn exit_scope(&mut self) {
        self.scope_depth = self.scope_depth.saturating_sub(1);
    }

    /// Records the expression id range checked for this function body.
    pub fn set_expr_range(&mut self, start: u32, end: u32) {
        self.expr_start = start;
        self.expr_end = end;
    }

    /// Allocates a slot to hold one `match` scrutinee for lowering.
    #[must_use]
    pub fn alloc_match_scrutinee_temp(&mut self, ty: TypeId) -> LocalSlot {
        let serial = self.match_temp_serial;
        self.match_temp_serial += 1;
        let symbol = Symbol::from_raw(0x9000_0000 | serial);
        let slot = self.alloc(symbol, ty, BindingKind::MatchTemp);
        self.match_temp_slots.push(slot);
        slot
    }

    /// Allocates a slot and records `symbol` with `ty` and `kind`.
    #[must_use]
    pub fn alloc(&mut self, symbol: Symbol, ty: TypeId, kind: BindingKind) -> LocalSlot {
        let slot = LocalSlot::from_raw(self.next_slot);
        self.next_slot += 1;
        self.bindings.push(Binding {
            symbol,
            slot,
            ty,
            kind,
            scope_depth: self.scope_depth,
        });
        slot
    }

    /// Finishes the layout.
    #[must_use]
    pub fn finish(self) -> FunctionLayout {
        FunctionLayout {
            def: self.def,
            return_type: self.return_type,
            bindings: self.bindings,
            match_temp_slots: self.match_temp_slots,
            expr_start: self.expr_start,
            expr_end: self.expr_end,
        }
    }
}

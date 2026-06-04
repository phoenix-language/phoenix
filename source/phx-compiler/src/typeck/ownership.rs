//! Use-after-move tracking (MVP).

use phx_diagnostics::Span;
use phx_syntax::Symbol;

use super::types::TypeId;

/// State of a local binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingState {
    /// Available for use.
    Valid,
    /// Moved; `move_span` is the move site.
    Moved(Span),
}

/// One local binding with the block depth where it was introduced.
#[derive(Debug, Clone)]
struct BindingEntry {
    symbol: Symbol,
    state: BindingState,
    ty: TypeId,
    depth: u32,
}

/// Tracks moves for locals in the current function/block scope.
#[derive(Debug, Default)]
pub struct OwnershipTracker {
    bindings: Vec<BindingEntry>,
    scope_depth: u32,
}

impl OwnershipTracker {
    /// Creates an empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enters a nested block scope (locals defined here are popped on [`Self::exit_scope`]).
    pub fn enter_scope(&mut self) {
        self.scope_depth = self.scope_depth.saturating_add(1);
    }

    /// Leaves a block scope and drops bindings introduced in that scope.
    pub fn exit_scope(&mut self) {
        if self.scope_depth == 0 {
            return;
        }
        self.scope_depth -= 1;
        self.bindings
            .retain(|entry| entry.depth <= self.scope_depth);
    }

    /// Registers a new binding as valid with its type at the current scope depth.
    pub fn define(&mut self, name: Symbol, ty: TypeId) {
        self.bindings.push(BindingEntry {
            symbol: name,
            state: BindingState::Valid,
            ty,
            depth: self.scope_depth,
        });
    }

    /// Returns the type recorded for `name` in the innermost active scope, if any.
    #[must_use]
    pub fn binding_type(&self, name: Symbol) -> Option<TypeId> {
        self.bindings
            .iter()
            .rfind(|entry| entry.symbol == name)
            .map(|entry| entry.ty)
    }

    /// Marks the innermost active binding for `name` as moved at `span`.
    pub fn move_binding(&mut self, name: Symbol, span: Span) {
        if let Some(entry) = self.bindings.iter_mut().rfind(|entry| entry.symbol == name) {
            entry.state = BindingState::Moved(span);
        }
    }

    /// Returns move span if the innermost active binding for `name` was moved.
    #[must_use]
    pub fn moved_at(&self, name: Symbol) -> Option<Span> {
        self.bindings
            .iter()
            .rfind(|entry| entry.symbol == name)
            .and_then(|entry| match entry.state {
                BindingState::Moved(span) => Some(span),
                BindingState::Valid => None,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phx_diagnostics::Span;

    #[test]
    fn moved_binding_reports_site() {
        let mut t = OwnershipTracker::new();
        let sym = Symbol::from_raw(1);
        t.define(sym, TypeId::from_raw(0));
        let move_span = Span::new(10, 11);
        t.move_binding(sym, move_span);
        assert_eq!(t.moved_at(sym), Some(move_span));
    }
}

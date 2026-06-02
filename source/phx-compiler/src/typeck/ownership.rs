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

/// Tracks moves for locals in the current function/block scope.
#[derive(Debug, Default)]
pub struct OwnershipTracker {
    bindings: Vec<(Symbol, BindingState, TypeId)>,
}

impl OwnershipTracker {
    /// Creates an empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new binding as valid with its type.
    pub fn define(&mut self, name: Symbol, ty: TypeId) {
        self.bindings.push((name, BindingState::Valid, ty));
    }

    /// Returns the type recorded for `name`, if any.
    #[must_use]
    pub fn binding_type(&self, name: Symbol) -> Option<TypeId> {
        self.bindings
            .iter()
            .find(|(s, _, _)| *s == name)
            .map(|(_, _, ty)| *ty)
    }

    /// Marks `name` as moved at `span`.
    pub fn move_binding(&mut self, name: Symbol, span: Span) {
        if let Some((_, state, _)) = self.bindings.iter_mut().find(|(s, _, _)| *s == name) {
            *state = BindingState::Moved(span);
        }
    }

    /// Returns move span if `name` was moved.
    #[must_use]
    pub fn moved_at(&self, name: Symbol) -> Option<Span> {
        self.bindings
            .iter()
            .find(|(s, _, _)| *s == name)
            .and_then(|(_, state, _)| match state {
                BindingState::Moved(span) => Some(*span),
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

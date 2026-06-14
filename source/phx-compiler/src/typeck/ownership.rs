//! Use-after-move tracking (MVP).

use phx_diagnostics::Span;
use phx_syntax::Symbol;

use super::types::TypeId;

/// State of a local binding.
///
/// MVP tracks whole-binding validity only; field-level partial moves are post-MVP.
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
#[derive(Debug, Clone, Default)]
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

    /// Joins ownership state after conditional arms: bindings visible at `base` are moved when
    /// moved on any arm (flow-insensitive MVP merge).
    #[must_use]
    pub fn join_arms(base: &Self, arm_ends: &[Self]) -> Self {
        let mut out = base.clone();
        let mut seen = std::collections::HashSet::new();
        for entry in base
            .bindings
            .iter()
            .rev()
            .filter(|entry| entry.depth <= base.scope_depth)
        {
            if !seen.insert(entry.symbol) {
                continue;
            }
            let symbol = entry.symbol;
            let depth = entry.depth;
            for arm in arm_ends {
                let Some(BindingState::Moved(span)) = arm
                    .bindings
                    .iter()
                    .find(|e| e.symbol == symbol && e.depth == depth)
                    .map(|e| e.state)
                else {
                    continue;
                };
                if let Some(out_entry) = out
                    .bindings
                    .iter_mut()
                    .find(|e| e.symbol == symbol && e.depth == depth)
                {
                    out_entry.state = BindingState::Moved(span);
                }
                break;
            }
        }
        out
    }

    fn binding_state_at_depth(&self, symbol: Symbol, depth: u32) -> Option<BindingState> {
        self.bindings
            .iter()
            .find(|entry| entry.symbol == symbol && entry.depth == depth)
            .map(|entry| entry.state)
    }

    /// Bindings visible at `base` that are [`BindingState::Moved`] in `joined` but valid in `base`.
    #[must_use]
    pub fn newly_moved_since(base: &Self, joined: &Self) -> Vec<(Symbol, u32, Span)> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for entry in base
            .bindings
            .iter()
            .rev()
            .filter(|entry| entry.depth <= base.scope_depth)
        {
            if !seen.insert(entry.symbol) {
                continue;
            }
            let symbol = entry.symbol;
            let depth = entry.depth;
            let Some(BindingState::Valid) = base.binding_state_at_depth(symbol, depth) else {
                continue;
            };
            let Some(BindingState::Moved(span)) = joined.binding_state_at_depth(symbol, depth)
            else {
                continue;
            };
            out.push((symbol, depth, span));
        }
        out
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

    #[test]
    fn join_arms_marks_moved_when_one_arm_moves() {
        let sym = Symbol::from_raw(1);
        let ty = TypeId::from_raw(0);
        let move_span = Span::new(5, 6);

        let mut base = OwnershipTracker::new();
        base.define(sym, ty);

        let mut arm_a = base.clone();
        arm_a.move_binding(sym, move_span);

        let arm_b = base.clone();

        let joined = OwnershipTracker::join_arms(&base, &[arm_a, arm_b]);
        assert_eq!(joined.moved_at(sym), Some(move_span));
    }

    #[test]
    fn join_arms_keeps_valid_when_no_arm_moves() {
        let sym = Symbol::from_raw(1);
        let ty = TypeId::from_raw(0);

        let mut base = OwnershipTracker::new();
        base.define(sym, ty);

        let joined = OwnershipTracker::join_arms(&base, &[base.clone(), base.clone()]);
        assert_eq!(joined.moved_at(sym), None);
    }

    #[test]
    fn join_arms_ignores_inner_scope_only_bindings() {
        let outer = Symbol::from_raw(1);
        let inner = Symbol::from_raw(2);
        let ty = TypeId::from_raw(0);
        let move_span = Span::new(1, 2);

        let mut base = OwnershipTracker::new();
        base.define(outer, ty);

        let mut arm = base.clone();
        arm.enter_scope();
        arm.define(inner, ty);
        arm.move_binding(inner, move_span);
        arm.exit_scope();

        let joined = OwnershipTracker::join_arms(&base, &[arm]);
        assert_eq!(joined.moved_at(outer), None);
        assert_eq!(joined.moved_at(inner), None);
    }

    #[test]
    fn newly_moved_since_reports_loop_carried_move() {
        let sym = Symbol::from_raw(1);
        let ty = TypeId::from_raw(0);
        let move_span = Span::new(5, 6);

        let mut base = OwnershipTracker::new();
        base.define(sym, ty);

        let mut body_end = base.clone();
        body_end.move_binding(sym, move_span);
        let joined = OwnershipTracker::join_arms(&base, &[body_end]);

        let newly = OwnershipTracker::newly_moved_since(&base, &joined);
        assert_eq!(newly, vec![(sym, 0, move_span)]);
    }

    #[test]
    fn newly_moved_since_skips_already_moved_in_base() {
        let sym = Symbol::from_raw(1);
        let ty = TypeId::from_raw(0);
        let move_span = Span::new(5, 6);

        let mut base = OwnershipTracker::new();
        base.define(sym, ty);
        base.move_binding(sym, move_span);

        let joined = base.clone();
        let newly = OwnershipTracker::newly_moved_since(&base, &joined);
        assert!(newly.is_empty());
    }

    #[test]
    fn newly_moved_since_ignores_inner_scope_only_bindings() {
        let outer = Symbol::from_raw(1);
        let inner = Symbol::from_raw(2);
        let ty = TypeId::from_raw(0);
        let move_span = Span::new(1, 2);

        let mut base = OwnershipTracker::new();
        base.define(outer, ty);

        let mut body_end = base.clone();
        body_end.enter_scope();
        body_end.define(inner, ty);
        body_end.move_binding(inner, move_span);
        body_end.exit_scope();

        let joined = OwnershipTracker::join_arms(&base, &[body_end]);
        let newly = OwnershipTracker::newly_moved_since(&base, &joined);
        assert!(newly.is_empty());
    }
}

//! Use-after-move tracking for the type checker (MVP).
//!
//! [`OwnershipTracker`] records whether each local binding is still valid after
//! moves. The expression and statement checkers call [`OwnershipTracker::move_binding`]
//! at move sites and [`OwnershipTracker::moved_at`] before reads; a moved binding
//! produces a [`TypeCheckError::UseAfterMove`](super::TypeCheckError::UseAfterMove)
//! diagnostic that cites the original move span.
//!
//! # MVP model
//!
//! Tracking is **whole-binding** only: a local is either [`BindingState::Valid`] or
//! [`BindingState::Moved`]. Field-level partial moves (invalidating only part of a
//! struct binding) are post-MVP; see `docs/design/features/ownership.md`.
//!
//! Copyable types skip this tracker entirely — their bindings are never marked moved.
//!
//! # Scope and shadowing
//!
//! Each binding is tagged with the block [`OwnershipTracker::scope_depth`] at
//! [`OwnershipTracker::define`]. [`OwnershipTracker::enter_scope`] /
//! [`OwnershipTracker::exit_scope`] mirror lexical blocks; exiting a scope drops
//! bindings introduced at deeper depths.
//!
//! Lookups (`binding_type`, `move_binding`, `moved_at`) resolve the **innermost**
//! active binding for a [`Symbol`] (last matching entry in the binding stack).
//!
//! # Control-flow merge
//!
//! Conditional arms and loop bodies fork from a shared pre-state, then merge:
//!
//! - [`OwnershipTracker::join_arms`] implements a flow-insensitive join: a binding
//!   visible at `base` becomes moved in the result if **any** arm end state moved it.
//! - [`OwnershipTracker::newly_moved_since`] diffs `base` against a joined head state
//!   to find bindings that became moved during a loop body — used to flag reads on
//!   the loop back edge as use-after-move.
//!
//! Bindings that exist only in inner scopes of an arm (never visible at `base`) are
//! ignored by both merge helpers.

use phx_diagnostics::Span;
use phx_syntax::Symbol;

use super::types::TypeId;

/// State of a local binding for use-after-move checking.
///
/// MVP tracks whole-binding validity only; field-level partial moves are post-MVP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingState {
    /// Binding may be read or moved again.
    Valid,
    /// Ownership was transferred; `move_span` is the site of the move for diagnostics.
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

/// Tracks move state for locals while type-checking a function body.
///
/// Invariants maintained by callers in `typeck::check`:
///
/// - `define` is called once per non-Copyable `let` binding at the current scope depth.
/// - `move_binding` is called at every move site (assignment RHS into an existing
///   binding, call argument pass-by-value, non-Copyable field move, etc.).
/// - `enter_scope` / `exit_scope` bracket every lexical block; `exit_scope` must
///   balance each `enter_scope`.
/// - After `if`/`match`, ownership is replaced with [`OwnershipTracker::join_arms`]
///   of the pre-condition snapshot and each arm's end state.
/// - Loop bodies use `join_arms(pre, [body_end])` as the head state and
///   [`OwnershipTracker::newly_moved_since`] to detect back-edge use-after-move.
#[derive(Debug, Clone, Default)]
pub struct OwnershipTracker {
    bindings: Vec<BindingEntry>,
    scope_depth: u32,
}

impl OwnershipTracker {
    /// Creates an empty tracker at scope depth zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enters a nested block scope.
    ///
    /// Bindings defined while this scope is active are removed by the matching
    /// [`Self::exit_scope`] call.
    pub fn enter_scope(&mut self) {
        self.scope_depth = self.scope_depth.saturating_add(1);
    }

    /// Leaves the current block scope and drops bindings introduced in that scope.
    ///
    /// If `scope_depth` is already zero, this is a no-op (defensive; well-formed
    /// checkers always balance enter/exit).
    pub fn exit_scope(&mut self) {
        if self.scope_depth == 0 {
            return;
        }
        self.scope_depth -= 1;
        self.bindings
            .retain(|entry| entry.depth <= self.scope_depth);
    }

    /// Registers a new binding as [`BindingState::Valid`] at the current scope depth.
    ///
    /// `ty` is the binding's declared type; it is returned by [`Self::binding_type`]
    /// until the binding leaves scope or is shadowed.
    pub fn define(&mut self, name: Symbol, ty: TypeId) {
        self.bindings.push(BindingEntry {
            symbol: name,
            state: BindingState::Valid,
            ty,
            depth: self.scope_depth,
        });
    }

    /// Returns the type recorded for the innermost active binding named `name`, if any.
    #[must_use]
    pub fn binding_type(&self, name: Symbol) -> Option<TypeId> {
        self.bindings
            .iter()
            .rfind(|entry| entry.symbol == name)
            .map(|entry| entry.ty)
    }

    /// Marks the innermost active binding for `name` as moved at `span`.
    ///
    /// No-op when `name` is not bound in an active scope (e.g. Copyable locals that
    /// were never registered).
    pub fn move_binding(&mut self, name: Symbol, span: Span) {
        if let Some(entry) = self.bindings.iter_mut().rfind(|entry| entry.symbol == name) {
            entry.state = BindingState::Moved(span);
        }
    }

    /// Returns the move site if the innermost active binding for `name` was moved.
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

    /// Joins ownership state after conditional arms.
    ///
    /// Starts from a clone of `base` and, for each binding visible at `base`'s
    /// scope depth (innermost shadow per symbol), marks it [`BindingState::Moved`]
    /// when **any** entry in `arm_ends` moved that symbol at the same depth.
    ///
    /// This is intentionally flow-insensitive: if one arm moves and another does
    /// not, the merged binding is treated as moved after the whole construct.
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

    /// Lists bindings that became moved in `joined` relative to `base`.
    ///
    /// Considers only bindings visible at `base`'s scope depth that were
    /// [`BindingState::Valid`] in `base` and [`BindingState::Moved`] in `joined`.
    /// Returns `(symbol, depth, move_span)` triples for loop back-edge checking.
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

//! Lexical scope stack for name resolution.
//!
//! ## Pass role
//!
//! Owned by the [`super::Resolver`] during the AST walk in [`super::walk`]. Tracks
//! which [`DefId`] is bound to each interned [`Symbol`] in the current lexical environment.
//!
//! ## Inputs and outputs
//!
//! - **Inputs:** binding introductions from the AST walk (`define_*`) and name uses (`lookup_*`).
//! - **Outputs:** [`DefId`] hits for successful lookups; duplicate bindings append
//!   [`ResolveError::DuplicateDefinition`] to a shared [`DiagnosticBag`] without aborting the pass.
//!
//! Value names (expressions, patterns) and type names (signatures, paths) live in separate maps per
//! [`Scope`]. Lookup walks from innermost to outermost scope; the module scope seeded by
//! [`super::walk`] seeds the outermost module scope before body resolution.

use std::collections::HashMap;

use phx_diagnostics::{DiagnosticBag, ResolveError, Span};
use phx_syntax::Symbol;

use super::def_id::{Def, DefId};

/// One lexical scope layer with separate value and type bindings.
///
/// Each map stores the most recent [`DefId`] for a [`Symbol`] introduced in this layer only.
/// Shadowing is represented by pushing a child [`Scope`] on [`ScopeStack`]; outer bindings remain
/// visible until the child is popped.
#[derive(Debug, Default)]
pub(crate) struct Scope {
    values: HashMap<Symbol, DefId>,
    types: HashMap<Symbol, DefId>,
}

/// Stack of nested scopes with the innermost scope at the end of the vector.
///
/// [`Resolver`](super::Resolver) pushes before block bodies, generic parameter lists, and closure
/// parameter lists, then pops when leaving. Module-level and import bindings live in the bottom
/// scope(s) and are not removed until the module walk finishes.
#[derive(Debug, Default)]
pub(crate) struct ScopeStack {
    scopes: Vec<Scope>,
}

impl ScopeStack {
    /// Pushes an empty scope layer for a new lexical block or signature list.
    ///
    /// Must be paired with [`Self::pop`] on all exit paths (including early returns in the walker).
    pub(crate) fn push(&mut self) {
        self.scopes.push(Scope::default());
    }

    /// Removes the innermost scope layer.
    ///
    /// Bindings defined only in that layer become invisible to subsequent [`Self::lookup_value`]
    /// and [`Self::lookup_type`] calls. Popping past the module scope is a resolver bug.
    pub(crate) fn pop(&mut self) {
        self.scopes.pop();
    }

    /// Registers a value-namespace binding in the innermost scope.
    ///
    /// On a duplicate name in the same layer, records [`ResolveError::DuplicateDefinition`] with the
    /// first defining span from `defs` and still overwrites the map entry so later lookups resolve
    /// to the latest binding.
    ///
    /// # Panics
    ///
    /// Never panics on user input. If the stack is empty, the binding is silently dropped (an
    /// internal invariant violation in the walker).
    pub(crate) fn define_value(
        &mut self,
        defs: &[Def],
        bag: &mut DiagnosticBag,
        module: u32,
        name: Symbol,
        def_id: DefId,
        span: Span,
    ) {
        let Some(scope) = self.scopes.last_mut() else {
            return;
        };
        if let Some(&first_id) = scope.values.get(&name) {
            bag.push(
                module,
                ResolveError::DuplicateDefinition {
                    symbol_index: name.index(),
                    first_span: defs.get(first_id.index() as usize).map_or(span, |d| d.span),
                    span,
                },
            );
        }
        scope.values.insert(name, def_id);
    }

    /// Registers a type-namespace binding in the innermost scope.
    ///
    /// Same duplicate-reporting and overwrite behavior as [`Self::define_value`], but uses the type
    /// map (generics, structs, traits, type aliases, etc.).
    ///
    /// # Panics
    ///
    /// Never panics on user input. If the stack is empty, the binding is silently dropped (an
    /// internal invariant violation in the walker).
    pub(crate) fn define_type(
        &mut self,
        defs: &[Def],
        bag: &mut DiagnosticBag,
        module: u32,
        name: Symbol,
        def_id: DefId,
        span: Span,
    ) {
        let Some(scope) = self.scopes.last_mut() else {
            return;
        };
        if let Some(&first_id) = scope.types.get(&name) {
            bag.push(
                module,
                ResolveError::DuplicateDefinition {
                    symbol_index: name.index(),
                    first_span: defs.get(first_id.index() as usize).map_or(span, |d| d.span),
                    span,
                },
            );
        }
        scope.types.insert(name, def_id);
    }

    /// Resolves a value-namespace name from innermost to outermost scope.
    ///
    /// Returns `None` when the name is not bound in any active layer (caller emits
    /// [`ResolveError::UnresolvedIdent`]).
    #[must_use]
    pub(crate) fn lookup_value(&self, name: Symbol) -> Option<DefId> {
        for scope in self.scopes.iter().rev() {
            if let Some(id) = scope.values.get(&name) {
                return Some(*id);
            }
        }
        None
    }

    /// Resolves a type-namespace name from innermost to outermost scope.
    ///
    /// Returns `None` when the name is not bound in any active layer (caller emits
    /// [`ResolveError::UnresolvedType`]).
    #[must_use]
    pub(crate) fn lookup_type(&self, name: Symbol) -> Option<DefId> {
        for scope in self.scopes.iter().rev() {
            if let Some(id) = scope.types.get(&name) {
                return Some(*id);
            }
        }
        None
    }

    /// Returns the number of active scope layers (module scope counts as one).
    ///
    /// Stored on each [`Def::scope_depth`] at introduction time for closure upvar detection.
    #[must_use]
    pub(crate) fn depth(&self) -> u32 {
        u32::try_from(self.scopes.len()).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use phx_diagnostics::Span;
    use phx_syntax::{Interner, Program, SourceFile};

    use super::*;
    use crate::resolver::def_id::DefKind;

    #[test]
    fn scope_push_pop_lookup() {
        let mut interner = Interner::new();
        let sym = interner.intern("a").expect("test intern");
        let _sf = SourceFile::new(
            Program {
                imports: vec![],
                items: vec![],
            },
            interner,
        );
        let mut defs = Vec::new();
        let mut bag = DiagnosticBag::new();
        let mut scopes = ScopeStack::default();
        scopes.push();
        let id = DefId::from_raw(0);
        defs.push(Def::new(DefKind::Local, sym, Span::new(0, 1), 0, false, 1));
        scopes.define_value(&defs, &mut bag, 0, sym, id, Span::new(0, 1));
        assert!(scopes.lookup_value(sym).is_some());
        scopes.pop();
        assert!(scopes.lookup_value(sym).is_none());
    }
}

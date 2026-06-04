//! Lexical scope stack for name lookup.
//!
//! Each [`Scope`] holds value and type maps keyed by [`Symbol`]. Lookup walks from innermost to
//! module scope.

use std::collections::HashMap;

use phx_diagnostics::{DiagnosticBag, ResolveError, Span};
use phx_syntax::Symbol;

use super::def_id::{Def, DefId};

/// One lexical scope layer.
#[derive(Debug, Default)]
pub(crate) struct Scope {
    values: HashMap<Symbol, DefId>,
    types: HashMap<Symbol, DefId>,
}

/// Stack of nested scopes (innermost last).
#[derive(Debug, Default)]
pub(crate) struct ScopeStack {
    scopes: Vec<Scope>,
}

impl ScopeStack {
    /// Pushes an empty scope.
    pub(crate) fn push(&mut self) {
        self.scopes.push(Scope::default());
    }

    /// Pops the innermost scope.
    pub(crate) fn pop(&mut self) {
        self.scopes.pop();
    }

    /// Inserts a value binding; reports duplicate definitions via `bag`.
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
            bag.push(module, ResolveError::DuplicateDefinition {
                symbol_index: name.index(),
                first_span: defs[first_id.index() as usize].span,
                span,
            });
        }
        scope.values.insert(name, def_id);
    }

    /// Inserts a type binding; reports duplicate definitions via `bag`.
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
            bag.push(module, ResolveError::DuplicateDefinition {
                symbol_index: name.index(),
                first_span: defs[first_id.index() as usize].span,
                span,
            });
        }
        scope.types.insert(name, def_id);
    }

    /// Looks up a value name from innermost to outermost scope.
    pub(crate) fn lookup_value(&self, name: Symbol) -> Option<DefId> {
        for scope in self.scopes.iter().rev() {
            if let Some(id) = scope.values.get(&name) {
                return Some(*id);
            }
        }
        None
    }

    /// Looks up a type name from innermost to outermost scope.
    pub(crate) fn lookup_type(&self, name: Symbol) -> Option<DefId> {
        for scope in self.scopes.iter().rev() {
            if let Some(id) = scope.types.get(&name) {
                return Some(*id);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
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
        defs.push(Def::new(DefKind::Local, sym, Span::new(0, 1), 0, false));
        scopes.define_value(&defs, &mut bag, 0, sym, id, Span::new(0, 1));
        assert!(scopes.lookup_value(sym).is_some());
        scopes.pop();
        assert!(scopes.lookup_value(sym).is_none());
    }
}

//! Name resolution for a single Phoenix source file.
//!
//! Builds a [`ResolvedProgram`]: definition table plus [`ResolutionKey`] → [`DefId`] for name uses.
//! The syntax AST is left unchanged; the type checker will read side tables and the [`Interner`].

mod def_id;
mod scopes;
mod walk;

use scopes::ScopeStack;

use std::collections::HashMap;

use phx_diagnostics::{DiagnosticBag, Span};
use phx_syntax::{Interner, Program, SourceFile, Symbol};

pub use def_id::{Def, DefId, DefKind};

/// Key for a name-use resolution entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolutionKey {
    /// Span start byte offset.
    pub start: u32,
    /// Span end byte offset.
    pub end: u32,
    /// Interned name at the use site.
    pub symbol: Symbol,
}

/// Result of resolving a [`SourceFile`].
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedProgram {
    /// Parsed program (unchanged).
    pub program: Program,
    /// Interner from parse.
    pub interner: Interner,
    /// All definitions in this unit.
    pub defs: Vec<Def>,
    /// Resolved uses keyed by span + symbol.
    pub resolutions: HashMap<ResolutionKey, DefId>,
}

/// Resolves names in `source` (single file, no `#import` loading).
///
/// # Errors
///
/// Returns a [`DiagnosticBag`] when any resolve errors were collected.
pub fn resolve(source: &SourceFile) -> Result<ResolvedProgram, DiagnosticBag> {
    let mut resolver = Resolver {
        source,
        defs: Vec::new(),
        scopes: ScopeStack::default(),
        bag: DiagnosticBag::new(),
        resolutions: HashMap::new(),
        main_fn: None,
    };
    resolver.resolve_program();
    if resolver.bag.has_errors() {
        return Err(resolver.bag);
    }
    Ok(ResolvedProgram {
        program: source.program.clone(),
        interner: source.interner.clone(),
        defs: resolver.defs,
        resolutions: resolver.resolutions,
    })
}

/// Mutable state for one resolve pass over `source`.
pub(crate) struct Resolver<'a> {
    /// Parsed file (program + interner).
    pub(crate) source: &'a SourceFile,
    pub(crate) defs: Vec<Def>,
    pub(crate) scopes: ScopeStack,
    pub(crate) bag: DiagnosticBag,
    pub(crate) resolutions: HashMap<ResolutionKey, DefId>,
    pub(crate) main_fn: Option<DefId>,
}

impl Resolver<'_> {
    /// Appends a [`Def`] and returns its [`DefId`].
    pub(crate) fn alloc_def(&mut self, kind: DefKind, name: Symbol, span: Span) -> DefId {
        let id = DefId::from_raw(u32::try_from(self.defs.len()).unwrap_or(u32::MAX));
        self.defs.push(Def { kind, name, span });
        id
    }

    /// Registers a value name in the current scope (records duplicates in the bag).
    pub(crate) fn define_value(&mut self, name: Symbol, span: Span, kind: DefKind) -> DefId {
        let id = self.alloc_def(kind, name, span);
        self.scopes
            .define_value(&self.defs, &mut self.bag, name, id, span);
        id
    }

    /// Registers a type name in the current scope (records duplicates in the bag).
    pub(crate) fn define_type(&mut self, name: Symbol, span: Span, kind: DefKind) -> DefId {
        let id = self.alloc_def(kind, name, span);
        self.scopes
            .define_type(&self.defs, &mut self.bag, name, id, span);
        id
    }

    /// Records a successful name resolution at `span` for later phases.
    pub(crate) fn record_resolution(&mut self, span: Span, symbol: Symbol, def_id: Option<DefId>) {
        if let Some(id) = def_id {
            self.resolutions.insert(
                ResolutionKey {
                    start: span.start,
                    end: span.end,
                    symbol,
                },
                id,
            );
        }
    }
}

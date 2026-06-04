//! Name resolution for a single Phoenix source file.
//!
//! Builds a [`ResolvedProgram`]: definition table plus [`ResolutionKey`] → [`DefId`] for name uses.
//! The syntax AST is left unchanged; the type checker will read side tables and the [`Interner`].

mod def_id;
pub(crate) mod scopes;
mod walk;

use scopes::ScopeStack;

use std::collections::HashMap;

use std::path::PathBuf;

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

/// One source module in a crate.
#[derive(Debug, Clone)]
pub struct SourceModule {
    /// Module id (dense index).
    pub id: u32,
    /// Logical path (`a::b`) for diagnostics.
    pub logical_path: String,
    /// Path to the `.phx` file.
    pub filesystem: PathBuf,
    /// Source text.
    pub source: String,
    /// Parsed AST for this file.
    pub program: Program,
}

/// Result of resolving a crate (one or more modules).
///
/// Exposes full AST and side tables for in-tree passes and tests. Not a stable public API surface
/// for external IDEs or tooling until a narrower facade is introduced.
#[derive(Debug, Clone)]
pub struct ResolvedProgram {
    /// Entry module program (root file).
    pub program: Program,
    /// All modules in the crate (topological order).
    pub modules: Vec<SourceModule>,
    /// Entry module id.
    pub root: u32,
    /// Interner from parse.
    pub interner: Interner,
    /// All definitions in this unit.
    pub defs: Vec<Def>,
    /// Resolved uses keyed by span + symbol.
    pub resolutions: HashMap<ResolutionKey, DefId>,
    /// Definition id of `main` when present and valid.
    pub main_fn: Option<DefId>,
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
        current_module: 0,
        root_module: 0,
        logical_path: "main",
        allow_imports: false,
        collect_only: false,
        import_bindings: Vec::new(),
    };
    resolver.resolve_program();
    if resolver.bag.has_errors() {
        return Err(resolver.bag);
    }
    let program = source.program.clone();
    let interner = source.interner.clone();
    let module = SourceModule {
        id: 0,
        logical_path: "main".to_owned(),
        filesystem: PathBuf::new(),
        source: String::new(),
        program: program.clone(),
    };
    Ok(ResolvedProgram {
        program,
        modules: vec![module],
        root: 0,
        interner,
        defs: resolver.defs,
        resolutions: resolver.resolutions,
        main_fn: resolver.main_fn,
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
    pub(crate) current_module: u32,
    pub(crate) root_module: u32,
    pub(crate) logical_path: &'a str,
    pub(crate) allow_imports: bool,
    pub(crate) collect_only: bool,
    pub(crate) import_bindings: Vec<(Symbol, DefId, bool, Span)>,
}

impl Resolver<'_> {
    /// Appends a [`Def`] and returns its [`DefId`].
    pub(crate) fn alloc_def(
        &mut self,
        kind: DefKind,
        name: Symbol,
        span: Span,
        exported: bool,
    ) -> DefId {
        let id = DefId::from_raw(u32::try_from(self.defs.len()).unwrap_or(u32::MAX));
        self.defs
            .push(Def::new(kind, name, span, self.current_module, exported));
        id
    }

    /// Registers a value name in the current scope (records duplicates in the bag).
    pub(crate) fn define_value(&mut self, name: Symbol, span: Span, kind: DefKind) -> DefId {
        let id = self.alloc_def(kind, name, span, false);
        self.scopes.define_value(
            &self.defs,
            &mut self.bag,
            self.current_module,
            name,
            id,
            span,
        );
        id
    }

    /// Registers a type name in the current scope (records duplicates in the bag).
    pub(crate) fn define_type(&mut self, name: Symbol, span: Span, kind: DefKind) -> DefId {
        let id = self.alloc_def(kind, name, span, false);
        self.scopes.define_type(
            &self.defs,
            &mut self.bag,
            self.current_module,
            name,
            id,
            span,
        );
        id
    }

    /// Registers a top-level exported value or type.
    pub(crate) fn define_exported(
        &mut self,
        name: Symbol,
        span: Span,
        kind: DefKind,
        exported: bool,
    ) -> DefId {
        let id = self.alloc_def(kind, name, span, exported);
        if is_type_kind(kind) {
            self.scopes.define_type(
                &self.defs,
                &mut self.bag,
                self.current_module,
                name,
                id,
                span,
            );
        } else {
            self.scopes.define_value(
                &self.defs,
                &mut self.bag,
                self.current_module,
                name,
                id,
                span,
            );
        }
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

fn is_type_kind(kind: DefKind) -> bool {
    matches!(
        kind,
        DefKind::Struct
            | DefKind::Enum
            | DefKind::TypeAlias
            | DefKind::Trait
            | DefKind::GenericParam
    )
}

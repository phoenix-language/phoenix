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

use phx_diagnostics::{DiagnosticBag, ResolveError, Span};
use phx_syntax::{AstNodeId, Interner, Program, SourceFile, Symbol};

use crate::modules::{LoadedModule, ModuleId, SourceText};
use crate::project::BuildLayout;
use crate::pxi::PxiType;

pub(crate) use def_id::DefIdOverflow;
pub use def_id::{Def, DefId, DefKind};

/// Program-wide context for resolving block-scoped `#import` directives.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProgramImportEnv<'a> {
    /// All modules in the loaded program.
    pub modules: &'a [LoadedModule],
    /// Logical path → module id.
    pub path_index: &'a HashMap<String, ModuleId>,
    /// Per-module export maps.
    pub exports: &'a [HashMap<Symbol, DefId>],
    /// All definitions collected in phase 1.
    pub defs: &'a [Def],
    /// Build layout when resolving under a project.
    pub layout: Option<&'a BuildLayout>,
    /// Workspace package name.
    pub workspace_name: &'a str,
    /// Path-dependency package names.
    pub dep_names: &'a [&'a str],
    /// Submodule visibility graph.
    pub submodules: &'a crate::modules::SubmoduleRegistry,
}

/// Key for a name-use resolution entry (module + parse-time [`AstNodeId`], not span alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolutionKey {
    /// Owning module (program-wide id).
    pub module: u32,
    /// AST node or identifier id at the use site (unique per module parse).
    pub node_id: AstNodeId,
}

/// Captured outer binding for a closure body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosureUpvar {
    /// Captured name.
    pub symbol: Symbol,
    /// Definition being captured.
    pub def_id: DefId,
}

/// Resolved closure metadata keyed by closure [`DefId`].
#[derive(Debug, Clone, Default)]
pub struct ClosureInfo {
    /// Parent closure when nested, if any.
    pub parent: Option<DefId>,
    /// Outer bindings referenced from the closure body.
    pub upvars: Vec<ClosureUpvar>,
}

/// One source module in a loaded program.
#[derive(Debug, Clone)]
pub struct SourceModule {
    /// Module id (dense index).
    pub id: u32,
    /// Logical path (`a::b`) for diagnostics.
    pub logical_path: String,
    /// Path to the `.phx` file.
    pub filesystem: PathBuf,
    /// Source text.
    pub source: SourceText,
    /// Parsed AST for this file.
    pub program: Program,
}

/// Result of resolving a loaded program (one or more modules).
///
/// Exposes full AST and side tables for in-tree passes and tests. Not a stable public API surface
/// for external IDEs or tooling until a narrower facade is introduced.
#[derive(Debug, Clone)]
pub struct ResolvedProgram {
    /// Entry module program (root file).
    pub program: Program,
    /// All modules in the loaded program (topological order).
    pub modules: Vec<SourceModule>,
    /// Entry module id.
    pub root: u32,
    /// Interner from parse.
    pub interner: Interner,
    /// All definitions in this unit.
    pub defs: Vec<Def>,
    /// Resolved uses keyed by AST node id.
    pub resolutions: HashMap<ResolutionKey, DefId>,
    /// Closure capture tables keyed by closure definition id.
    pub closures: HashMap<DefId, ClosureInfo>,
    /// Definition id of `main` when present and valid.
    pub main_fn: Option<DefId>,
    /// Structured types from fresh dependency `.pxi` v2 (imported `DefId` → type).
    pub import_types: std::collections::HashMap<DefId, crate::pxi::PxiType>,
    /// Language item markers from dependency `.pxi` (imported `DefId` → marker).
    pub import_lang_items: std::collections::HashMap<DefId, crate::lang_items::LangItemMarker>,
    /// Item attribute metadata keyed by definition id.
    pub def_attrs: crate::attrs::DefAttrs,
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
        closures: HashMap::new(),
        closure_stack: Vec::new(),
        trait_impls: Vec::new(),
        main_fn: None,
        current_module: 0,
        root_module: 0,
        logical_path: "main",
        allow_imports: false,
        collect_only: false,
        import_bindings: Vec::new(),
        self_type_depth: 0,
        import_env: None,
        shared_interner: None,
        import_types: None,
        import_lang_items: None,
        def_attrs: crate::attrs::DefAttrs::new(),
        def_table_full: false,
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
        source: SourceText::from(""),
        program: program.clone(),
    };
    Ok(ResolvedProgram {
        program,
        modules: vec![module],
        root: 0,
        interner,
        defs: resolver.defs,
        resolutions: resolver.resolutions,
        closures: resolver.closures,
        main_fn: resolver.main_fn,
        import_types: HashMap::new(),
        import_lang_items: HashMap::new(),
        def_attrs: resolver.def_attrs,
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
    pub(crate) closures: HashMap<DefId, ClosureInfo>,
    /// Active closure defs (innermost last) while resolving lambda bodies.
    pub(crate) closure_stack: Vec<DefId>,
    /// `(type, optional trait type)` pairs for overlapping trait-impl detection.
    pub(crate) trait_impls: Vec<(Symbol, Option<phx_syntax::ast::Type>, Span)>,
    pub(crate) main_fn: Option<DefId>,
    pub(crate) current_module: u32,
    pub(crate) root_module: u32,
    pub(crate) logical_path: &'a str,
    pub(crate) allow_imports: bool,
    pub(crate) collect_only: bool,
    pub(crate) import_bindings: Vec<(Symbol, DefId, bool, Span)>,
    /// Nesting depth where `Self` is a valid type name (trait / impl method signatures).
    pub(crate) self_type_depth: u32,
    /// Loaded-program context for block-scoped imports (phase 2 only).
    pub(crate) import_env: Option<ProgramImportEnv<'a>>,
    /// Shared interner for cross-module import resolution.
    pub(crate) shared_interner: Option<&'a mut Interner>,
    /// Imported type table from dependency `.pxi` files.
    pub(crate) import_types: Option<&'a mut HashMap<DefId, PxiType>>,
    /// Language item markers from dependency `.pxi` files.
    pub(crate) import_lang_items: Option<&'a mut HashMap<DefId, crate::lang_items::LangItemMarker>>,
    /// Item attribute metadata collected during definition collection.
    pub(crate) def_attrs: crate::attrs::DefAttrs,
    /// Set after the definition table exceeds `u32::MAX`; further defs are skipped.
    pub(crate) def_table_full: bool,
}

impl Resolver<'_> {
    /// Appends a [`Def`] and returns its [`DefId`].
    ///
    /// Returns `None` when the definition table is full ([`ResolveError::ProgramTooLarge`]).
    pub(crate) fn alloc_def(
        &mut self,
        kind: DefKind,
        name: Symbol,
        span: Span,
        exported: bool,
    ) -> Option<DefId> {
        if self.def_table_full {
            return None;
        }
        let Ok(id) = DefId::try_from_index(self.defs.len()) else {
            self.bag
                .push(self.current_module, ResolveError::ProgramTooLarge { span });
            self.def_table_full = true;
            return None;
        };
        self.defs.push(Def::new(
            kind,
            name,
            span,
            self.current_module,
            exported,
            self.scopes.depth(),
        ));
        Some(id)
    }

    /// Registers a value name in the current scope (records duplicates in the bag).
    pub(crate) fn define_value(
        &mut self,
        name: Symbol,
        span: Span,
        kind: DefKind,
    ) -> Option<DefId> {
        let id = self.alloc_def(kind, name, span, false)?;
        self.scopes.define_value(
            &self.defs,
            &mut self.bag,
            self.current_module,
            name,
            id,
            span,
        );
        Some(id)
    }

    /// Registers a type name in the current scope (records duplicates in the bag).
    pub(crate) fn define_type(&mut self, name: Symbol, span: Span, kind: DefKind) -> Option<DefId> {
        let id = self.alloc_def(kind, name, span, false)?;
        self.scopes.define_type(
            &self.defs,
            &mut self.bag,
            self.current_module,
            name,
            id,
            span,
        );
        Some(id)
    }

    /// Registers a top-level exported value or type.
    pub(crate) fn define_exported(
        &mut self,
        name: Symbol,
        span: Span,
        kind: DefKind,
        exported: bool,
    ) -> Option<DefId> {
        let id = self.alloc_def(kind, name, span, exported)?;
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
        Some(id)
    }

    /// Stores item attribute metadata for `def_id`.
    pub(crate) fn record_def_attrs(&mut self, def_id: DefId, attrs: crate::attrs::ItemAttrs) {
        if attrs.deprecated.is_some() || attrs.must_use {
            self.def_attrs.insert(def_id, attrs);
        }
    }

    /// Records a successful name resolution at `node_id` for later phases.
    pub(crate) fn record_resolution(&mut self, node_id: AstNodeId, def_id: Option<DefId>) {
        let Some(id) = def_id else {
            return;
        };
        self.resolutions.insert(
            ResolutionKey {
                module: self.current_module,
                node_id,
            },
            id,
        );
        let Some(&closure_id) = self.closure_stack.last() else {
            return;
        };
        let Some(def) = self.defs.get(id.index() as usize) else {
            return;
        };
        if def.scope_depth < self.scopes.depth() {
            let Some(symbol) = symbol_from_def(&self.defs, id) else {
                return;
            };
            self.record_upvar(closure_id, symbol, id);
        }
    }

    fn record_upvar(&mut self, closure_id: DefId, symbol: Symbol, def_id: DefId) {
        let info = self
            .closures
            .entry(closure_id)
            .or_insert_with(|| ClosureInfo {
                parent: self.closure_stack.iter().rev().nth(1).copied(),
                upvars: Vec::new(),
            });
        if !info.upvars.iter().any(|u| u.def_id == def_id) {
            info.upvars.push(ClosureUpvar { symbol, def_id });
        }
    }
}

fn symbol_from_def(defs: &[Def], id: DefId) -> Option<Symbol> {
    defs.get(id.index() as usize).map(|d| d.name)
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

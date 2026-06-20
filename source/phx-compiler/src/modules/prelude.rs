//! Implicit prelude bindings when `[project] prelude = true` and std is linked.
//!
//! ## Pass role
//!
//! Injects a fixed set of `std::core::*` names into workspace modules during
//! [`super::resolve_loaded_program`] when [`LoadedProgram::prelude_enabled`] is set. Bindings
//! are resolved against the loaded `std` package's export maps, not hard-coded def ids.
//!
//! ## When prelude applies
//!
//! Prelude injection runs in [`super::resolve_loaded_program::build_import_bindings`] only when
//! all of the following hold:
//!
//! 1. The loaded program has `prelude_enabled` (from `[project] prelude = true` in `phoenix.toml`).
//! 2. The target module belongs to the workspace package (not a path-dependency root).
//! 3. The corresponding `std::core::*` submodule was loaded and exports the name.
//! 4. The importer has not already bound the name via an explicit `#import` (checked via `seen`).
//!
//! Missing std modules or exports are skipped silently — prelude is best-effort and does not
//! emit diagnostics when `std` is absent or incomplete.
//!
//! ## Catalog (V0-044)
//!
//! [`PRELUDE_ITEMS`] lists the minimal surface: `Option`/`Some`/`None`, `Result`/`Ok`/`Err`,
//! and core traits (`Copyable`, `Clone`, `Drop`, `PartialEq`, `Eq`, `Debug`, `Display`). Each
//! entry names a logical module path and whether the binding occupies the type namespace.
//!
//! ## Resolution flow
//!
//! 1. Iterate [`PRELUDE_ITEMS`] in declaration order.
//! 2. Skip names already present in `seen` (explicit imports win).
//! 3. Look up the std submodule in `path_index`; skip if not loaded.
//! 4. Find the exported symbol in that module's export map via the interner.
//! 5. Push `(symbol, def_id, is_type, synthetic_span)` into the import preface.

use std::collections::{HashMap, HashSet};

use phx_diagnostics::Span;
use phx_syntax::{Interner, Symbol};

use super::loader::ModuleId;
use crate::resolver::DefId;

/// One symbol injected into module scope from a std submodule export.
///
/// Static metadata only — actual [`DefId`] values come from the loaded program's export maps at
/// resolve time so prelude stays correct across std rebuilds.
struct PreludeItem {
    /// Logical module path (e.g. `std::core::option`).
    module: &'static str,
    /// Exported symbol name.
    name: &'static str,
    /// `true` when the binding is a type name (type namespace); `false` for value constructors.
    is_type: bool,
}

/// Minimal prelude surface (V0-044).
///
/// Order is stable but not semantically significant; duplicate-name skipping happens per item
/// via the `seen` set in [`prelude_bindings`].
const PRELUDE_ITEMS: &[PreludeItem] = &[
    PreludeItem {
        module: "std::core::option",
        name: "Option",
        is_type: true,
    },
    PreludeItem {
        module: "std::core::option",
        name: "Some",
        is_type: false,
    },
    PreludeItem {
        module: "std::core::option",
        name: "None",
        is_type: false,
    },
    PreludeItem {
        module: "std::core::result",
        name: "Result",
        is_type: true,
    },
    PreludeItem {
        module: "std::core::result",
        name: "Ok",
        is_type: false,
    },
    PreludeItem {
        module: "std::core::result",
        name: "Err",
        is_type: false,
    },
    PreludeItem {
        module: "std::core::copyable",
        name: "Copyable",
        is_type: true,
    },
    PreludeItem {
        module: "std::core::clone",
        name: "Clone",
        is_type: true,
    },
    PreludeItem {
        module: "std::core::drop",
        name: "Drop",
        is_type: true,
    },
    PreludeItem {
        module: "std::core::cmp",
        name: "PartialEq",
        is_type: true,
    },
    PreludeItem {
        module: "std::core::cmp",
        name: "Eq",
        is_type: true,
    },
    PreludeItem {
        module: "std::core::fmt",
        name: "Debug",
        is_type: true,
    },
    PreludeItem {
        module: "std::core::fmt_display",
        name: "Display",
        is_type: true,
    },
];

/// Context for resolving prelude exports from a loaded program.
///
/// Read-only views of the same tables [`super::resolve_loaded_program`] uses after phase 1
/// export collection. Constructed per module in
/// [`super::resolve_loaded_program::build_import_bindings`].
pub(crate) struct PreludeCtx<'a> {
    /// Logical path → module id (from [`super::loader::LoadedProgram::path_index`]).
    pub path_index: &'a HashMap<String, ModuleId>,
    /// Per-module export maps (symbol → def id) after phase 1 and reexports.
    pub exports: &'a [HashMap<Symbol, DefId>],
    /// Interner for symbol name comparison.
    pub interner: &'a Interner,
    /// Synthetic span attached to injected bindings (prelude has no source site).
    pub span: Span,
}

/// Returns `true` when any symbol in `seen` resolves to `name` via the interner.
///
/// Explicit `#import` bindings are recorded in `seen` before prelude runs so user imports take
/// precedence over implicit std names.
fn seen_contains_name(seen: &HashSet<Symbol>, interner: &Interner, name: &str) -> bool {
    seen.iter().any(|sym| interner.resolves_to(*sym, name))
}

/// Builds implicit prelude bindings, skipping symbols already imported explicitly.
///
/// Walks [`PRELUDE_ITEMS`] and resolves each name against the loaded std export maps. Returns
/// `(local_symbol, def_id, is_type_namespace, span)` tuples in the same shape as
/// [`super::import_resolve::resolve_import_directive`], ready for
/// [`crate::resolver::Resolver::import_bindings`].
///
/// Items whose std submodule is missing or does not export the name are omitted without
/// diagnostics — callers treat prelude as optional sugar when std is linked.
///
/// # Panics
///
/// Never panics on malformed user input; out-of-range module indices are skipped.
#[must_use]
pub(crate) fn prelude_bindings(
    ctx: &PreludeCtx<'_>,
    seen: &HashSet<Symbol>,
) -> Vec<(Symbol, DefId, bool, Span)> {
    let mut bindings = Vec::new();
    for item in PRELUDE_ITEMS {
        if seen_contains_name(seen, ctx.interner, item.name) {
            continue;
        }
        let Some(&mod_id) = ctx.path_index.get(item.module) else {
            continue;
        };
        let idx = mod_id.index() as usize;
        if idx >= ctx.exports.len() {
            continue;
        }
        let Some((sym, def_id)) = ctx.exports[idx].iter().find_map(|(sym, id)| {
            if ctx.interner.resolves_to(*sym, item.name) {
                Some((*sym, *id))
            } else {
                None
            }
        }) else {
            continue;
        };
        bindings.push((sym, def_id, item.is_type, ctx.span));
    }
    bindings
}

//! Implicit prelude bindings when `[project] prelude = true` and std is linked.

use std::collections::{HashMap, HashSet};

use phx_diagnostics::Span;
use phx_syntax::{Interner, Symbol};

use super::loader::ModuleId;
use crate::resolver::DefId;

/// One symbol injected into module scope from a std submodule export.
struct PreludeItem {
    /// Logical module path (e.g. `std::core::option`).
    module: &'static str,
    /// Exported symbol name.
    name: &'static str,
    /// `true` when the binding is a type name.
    is_type: bool,
}

/// Minimal prelude surface (V0-044).
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
];

/// Context for resolving prelude exports from a loaded program.
pub(crate) struct PreludeCtx<'a> {
    /// Logical path → module id.
    pub path_index: &'a HashMap<String, ModuleId>,
    /// Per-module export maps (symbol → def id).
    pub exports: &'a [HashMap<Symbol, DefId>],
    /// Interner for symbol lookup.
    pub interner: &'a Interner,
    /// Synthetic span for injected bindings.
    pub span: Span,
}

fn seen_contains_name(seen: &HashSet<Symbol>, interner: &Interner, name: &str) -> bool {
    seen.iter().any(|sym| interner.resolve(*sym) == name)
}

/// Builds implicit prelude bindings, skipping symbols already imported explicitly.
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
            if ctx.interner.resolve(*sym) == item.name {
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

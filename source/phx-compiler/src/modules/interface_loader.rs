//! Import scope from dependency `.pxi` files (separate compilation).

use std::collections::HashMap;
use std::path::Path;

use phx_syntax::Interner;
use phx_syntax::Symbol;

use crate::project::BuildLayout;
use crate::pxi::{PxiError, PxiFile};
use crate::resolver::DefId;

use super::graph::import_target_module;
use super::loader::{LoadedModule, ModuleId};

/// Import binding from a `.pxi` export.
pub type PxiBinding = (Symbol, DefId, bool);

/// Error loading interfaces for imports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterfaceLoadError {
    /// `.pxi` missing or stale.
    Stale {
        /// Dependency module path.
        module: String,
        /// Detail.
        message: String,
    },
    /// `.pxi` parse failure.
    Pxi(PxiError),
}

impl std::fmt::Display for InterfaceLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stale { module, message } => {
                write!(f, "stale interface for `{module}`: {message}")
            }
            Self::Pxi(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for InterfaceLoadError {}

/// Builds import preface bindings from dependency `.pxi` files.
///
/// # Errors
///
/// Returns [`InterfaceLoadError`] when a dependency interface is missing or stale.
pub fn bindings_from_pxi(
    module: &LoadedModule,
    layout: &BuildLayout,
    path_index: &HashMap<String, ModuleId>,
    sources: &HashMap<String, &Path>,
    interner: &mut Interner,
    mut next_def_index: u32,
) -> Result<Vec<PxiBinding>, InterfaceLoadError> {
    let mut bindings = Vec::new();

    for imp in phx_syntax::all_imports(&module.program) {
        let target = import_target_module(&imp.inner, interner);
        let key = target.display();
        if key.is_empty() || !path_index.contains_key(&key) {
            continue;
        }
        let pxi_path = layout.module_artifacts(&key).pxi;
        let pxi = PxiFile::read_from_path(&pxi_path).map_err(InterfaceLoadError::Pxi)?;
        if let Some(src) = sources.get(&key) {
            if !pxi.source_is_fresh(src) {
                return Err(InterfaceLoadError::Stale {
                    module: key,
                    message: "`.pxi` does not match source".to_owned(),
                });
            }
        } else if !pxi_path.is_file() {
            return Err(InterfaceLoadError::Stale {
                module: key,
                message: "missing `.pxi`".to_owned(),
            });
        }
        for exp in &pxi.exports {
            let Some(sym) = interner.intern(&exp.name).ok() else {
                continue;
            };
            let id = DefId::from_raw(next_def_index);
            next_def_index = next_def_index.saturating_add(1);
            let is_type = matches!(exp.kind.as_str(), "struct" | "enum" | "type" | "trait");
            bindings.push((sym, id, is_type));
        }
    }
    Ok(bindings)
}

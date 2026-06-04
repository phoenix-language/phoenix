//! Emit `.pxi` from a typed crate.

use std::collections::HashMap;
use std::path::Path;

use phx_syntax::Interner;

use crate::modules::{LoadedModule, import_target_module};
use crate::project::BuildLayout;
use crate::pxi::{PxiDependency, PxiExport, PxiFile, def_kind_to_pxi, digest_bytes, digest_file};
use crate::resolver::{Def, DefId, DefKind};
use crate::typeck::BindingKind;
use crate::typeck::{TypedProgram, format_type};

/// Builds a [`PxiFile`] for one module in a typed crate.
#[must_use]
pub fn build_pxi_for_module(
    logical_path: &str,
    source_path: &Path,
    module_id: u32,
    typed: &TypedProgram,
    exports: &HashMap<phx_syntax::Symbol, DefId>,
    dependencies: &[PxiDependency],
) -> PxiFile {
    let source_hash = digest_file(source_path).unwrap_or_else(|_| digest_bytes(b""));
    let interner = &typed.resolved.interner;
    let mut pxi_exports = Vec::new();

    for (sym, &def_id) in exports {
        let Some(def) = typed.resolved.defs.get(def_id.index() as usize) else {
            continue;
        };
        if def.module != module_id || !def.exported {
            continue;
        }
        let signature = export_signature(def, def_id, typed, interner);
        pxi_exports.push(PxiExport {
            name: interner.resolve(*sym).to_owned(),
            kind: def_kind_to_pxi(def.kind).to_owned(),
            signature,
        });
    }
    pxi_exports.sort_by(|a, b| a.name.cmp(&b.name));

    PxiFile {
        format_version: 1,
        logical_module: logical_path.to_owned(),
        source_hash,
        origin: None,
        exports: pxi_exports,
        dependencies: dependencies.to_vec(),
    }
}

fn export_signature(def: &Def, def_id: DefId, typed: &TypedProgram, names: &Interner) -> String {
    match def.kind {
        DefKind::Fn => typed
            .functions
            .iter()
            .find(|f| f.def == def_id).map_or_else(|| "() => ()".to_owned(), |f| {
                let params: Vec<_> = f
                    .bindings
                    .iter()
                    .filter(|b| b.kind == BindingKind::Param)
                    .map(|p| format_type(&typed.types, names, &typed.resolved.defs, p.ty))
                    .collect();
                let ret = format_type(&typed.types, names, &typed.resolved.defs, f.return_type);
                format!("({}) => {}", params.join(", "), ret)
            }),
        DefKind::Struct | DefKind::Enum | DefKind::TypeAlias => typed
            .layout
            .type_id(def_id).map_or_else(|| def_kind_to_pxi(def.kind).to_owned(), |tid| format!("type_id({tid})")),
        _ => def_kind_to_pxi(def.kind).to_owned(),
    }
}

/// Collects direct import dependencies with current `.pxi` hashes.
pub fn module_dependencies(
    module: &LoadedModule,
    layout: &BuildLayout,
    path_index: &HashMap<String, crate::modules::ModuleId>,
    interner: &Interner,
    workspace_name: &str,
    dep_names: &[&str],
) -> Vec<PxiDependency> {
    let mut deps = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for imp in &module.program.imports {
        let target = import_target_module(&imp.inner, interner);
        let canonical =
            crate::modules::ModulePath::canonicalize_import(&target, workspace_name, dep_names);
        let key = canonical.display();
        if key.is_empty() || !path_index.contains_key(&key) {
            continue;
        }
        if !seen.insert(key.clone()) {
            continue;
        }
        let pxi_path = layout.module_artifacts(&key).pxi;
        let pxi_hash = std::fs::read_to_string(&pxi_path).map_or_else(|_| String::new(), |t| digest_bytes(t.as_bytes()));
        deps.push(PxiDependency {
            logical_module: key,
            pxi_hash,
        });
    }
    deps
}

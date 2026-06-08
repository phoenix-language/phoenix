//! Emit `.pxi` from a typed crate.

use std::collections::HashMap;
use std::path::Path;

use phx_syntax::Interner;

use crate::modules::{LoadedModule, import_target_module};
use crate::project::BuildLayout;
use crate::pxi::serialize_ty::{
    enum_export_type, fn_export_type, signature_string, struct_export_type,
};
use crate::pxi::{
    PxiDependency, PxiExport, PxiFile, def_kind_to_pxi, digest_bytes, digest_file, stable_export_id,
};
use crate::resolver::{Def, DefId, DefKind};
use crate::typeck::BindingKind;
use crate::typeck::TypedProgram;
use crate::typeck::{is_generic_fn_template, mangle_export_id};

/// Builds a [`PxiFile`] (format v2) for one module in a typed crate.
#[must_use]
pub fn build_pxi_for_module(
    logical_path: &str,
    source_path: &Path,
    module_id: u32,
    typed: &TypedProgram,
    exports: &HashMap<phx_syntax::Symbol, DefId>,
    dependencies: &[PxiDependency],
    global_fn: Option<&HashMap<DefId, u32>>,
) -> PxiFile {
    let source_hash = digest_file(source_path).unwrap_or_else(|_| digest_bytes(b""));
    let interner = &typed.resolved.interner;
    let defs = &typed.resolved.defs;
    let mut pxi_exports = Vec::new();
    let mut emitted = std::collections::HashSet::new();

    for (sym, &def_id) in exports {
        let Some(def) = defs.get(def_id.index() as usize) else {
            continue;
        };
        if def.module != module_id || !def.exported {
            continue;
        }
        push_export(
            &mut pxi_exports,
            &mut emitted,
            logical_path,
            def,
            def_id,
            typed,
            interner,
            global_fn,
            interner.resolve(*sym),
        );
    }

    for (i, def) in defs.iter().enumerate() {
        if def.module != module_id || !def.exported {
            continue;
        }
        let def_id = DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX));
        if !typed.specialized_from.contains_key(&def_id) {
            continue;
        }
        push_export(
            &mut pxi_exports,
            &mut emitted,
            logical_path,
            def,
            def_id,
            typed,
            interner,
            global_fn,
            interner.resolve(def.name),
        );
    }

    // Trait/inherent impl methods are module-private but must appear in `.pxi` so
    // dependents can link associated fns (e.g. `From::from` in `std::error::from_io`).
    for (i, def) in defs.iter().enumerate() {
        if def.module != module_id || def.exported || def.kind != DefKind::Fn {
            continue;
        }
        let def_id = DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX));
        push_export(
            &mut pxi_exports,
            &mut emitted,
            logical_path,
            def,
            def_id,
            typed,
            interner,
            global_fn,
            interner.resolve(def.name),
        );
    }
    pxi_exports.sort_by(|a, b| a.name.cmp(&b.name));

    PxiFile {
        format_version: 2,
        logical_module: logical_path.to_owned(),
        source_hash,
        origin: None,
        exports: pxi_exports,
        dependencies: dependencies.to_vec(),
    }
}

#[allow(clippy::too_many_arguments)]
fn push_export(
    pxi_exports: &mut Vec<PxiExport>,
    emitted: &mut std::collections::HashSet<String>,
    logical_path: &str,
    def: &Def,
    def_id: DefId,
    typed: &TypedProgram,
    interner: &Interner,
    global_fn: Option<&HashMap<DefId, u32>>,
    name: &str,
) {
    if !emitted.insert(name.to_owned()) {
        return;
    }
    let kind = def_kind_to_pxi(def.kind).to_owned();
    let ty = export_structured_type(def, def_id, typed, logical_path);
    let signature = export_signature_fallback(def, def_id, typed, interner);
    let function_id = if kind == "fn" {
        if is_generic_fn_template(typed, def_id) {
            None
        } else {
            global_fn.and_then(|map| map.get(&def_id).copied())
        }
    } else {
        None
    };
    pxi_exports.push(PxiExport {
        export_id: if typed.specialized_from.contains_key(&def_id) {
            mangle_export_id(logical_path, &kind, name)
        } else {
            stable_export_id(logical_path, name, &kind)
        },
        name: name.to_owned(),
        kind,
        signature,
        ty,
        function_id,
    });
}

fn export_structured_type(
    def: &Def,
    def_id: DefId,
    typed: &TypedProgram,
    logical_module: &str,
) -> Option<super::type_ast::PxiType> {
    let interner = &typed.resolved.interner;
    let defs = &typed.resolved.defs;
    let layout = &typed.layout;
    let ty_interner = &typed.types;
    match def.kind {
        DefKind::Fn => typed.functions.iter().find(|f| f.def == def_id).map(|f| {
            let params: Vec<_> = f
                .bindings
                .iter()
                .filter(|b| b.kind == BindingKind::Param)
                .map(|p| p.ty)
                .collect();
            fn_export_type(
                ty_interner,
                interner,
                defs,
                layout,
                logical_module,
                &params,
                f.return_type,
            )
        }),
        DefKind::Struct => layout.structs.get(&def_id).map(|sl| {
            struct_export_type(
                ty_interner,
                interner,
                defs,
                layout,
                logical_module,
                def_id,
                sl,
            )
        }),
        DefKind::Enum => layout
            .enums
            .get(&def_id)
            .map(|el| enum_export_type(ty_interner, interner, defs, layout, logical_module, el)),
        DefKind::TypeAlias => Some(super::type_ast::PxiType::Named {
            path: format!("{logical_module}::{}", interner.resolve(def.name)),
            args: vec![],
        }),
        _ => None,
    }
}

fn export_signature_fallback(
    def: &Def,
    def_id: DefId,
    typed: &TypedProgram,
    names: &Interner,
) -> String {
    match def.kind {
        DefKind::Fn => typed
            .functions
            .iter()
            .find(|f| f.def == def_id)
            .map_or_else(
                || "() => ()".to_owned(),
                |f| {
                    let params: Vec<_> = f
                        .bindings
                        .iter()
                        .filter(|b| b.kind == BindingKind::Param)
                        .map(|p| signature_string(&typed.types, names, &typed.resolved.defs, p.ty))
                        .collect();
                    let ret =
                        signature_string(&typed.types, names, &typed.resolved.defs, f.return_type);
                    format!("({}) => {}", params.join(", "), ret)
                },
            ),
        DefKind::Struct | DefKind::Enum | DefKind::TypeAlias => names.resolve(def.name).to_owned(),
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
    for imp in phx_syntax::all_imports(&module.program) {
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
        let pxi_path = layout
            .module_artifacts_resolved(&key, workspace_name, dep_names)
            .pxi;
        let pxi_hash = std::fs::read_to_string(&pxi_path)
            .map_or_else(|_| String::new(), |t| digest_bytes(t.as_bytes()));
        deps.push(PxiDependency {
            logical_module: key,
            pxi_hash,
        });
    }
    deps
}

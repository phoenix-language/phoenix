//! Emit `.pxi` interface files from a type-checked module.
//!
//! ## Pass role
//!
//! Called by the build driver after resolve and type-check. Walks `pub` exports (plus
//! specialized generic exports and trait/inherent impl methods required for linking) and
//! produces a format-v2 [`PxiFile`] with structured types, stable export ids, optional
//! `function_id` values, and direct-import dependency hashes.
//!
//! ## Export selection
//!
//! [`build_pxi_for_module`] includes:
//!
//! - Every `pub` item listed in the module's export map
//! - Monomorphized generic exports recorded in [`TypedProgram::specialized_from`](crate::typeck::TypedProgram::specialized_from)
//! - Non-`pub` function bodies on trait/inherent impls (so dependents can link associated fns)
//!
//! Generic templates omit `function_id`; concrete specializations include the global PHX0 id
//! when `global_fn` is supplied.

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
use crate::typeck::{is_generic_fn_template, is_generic_impl_method_template, mangle_export_id};

/// Builds a format-v2 [`PxiFile`] for one module in a typed program.
///
/// `logical_path` becomes [`PxiFile::logical_module`]. `source_path` is hashed into
/// [`PxiFile::source_hash`]. `module_id` selects defs belonging to this module.
/// `exports` maps exported symbol names to [`DefId`] values from resolve.
/// `dependencies` are copied verbatim (typically from [`module_dependencies`]).
/// When `global_fn` is present, non-template `fn` exports receive a PHX0 `function_id`.
///
/// # Panics
///
/// Never panics on user input; missing source files yield an empty digest for `source_hash`.
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
        if !def.exported {
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
            interner.resolve(*sym).unwrap_or("<?>"),
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
            interner.resolve(def.name).unwrap_or("<?>"),
        );
    }

    // Trait/inherent impl methods are module-private but must appear in `.pxi` so
    // dependents can link associated fns (e.g. user `From::from` impls).
    for (i, def) in defs.iter().enumerate() {
        if def.module != module_id || def.exported || !def.kind.is_function_body() {
            continue;
        }
        let def_id = DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX));
        let name = interner.resolve(def.name).unwrap_or("<?>");
        if name == "fmt"
            && crate::typeck::is_builtin_type_impl_method(typed, def_id)
            && !crate::typeck::is_str_builtin_impl_method(typed, def_id)
        {
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
            interner.resolve(def.name).unwrap_or("<?>"),
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
        if is_generic_fn_template(typed, def_id) || is_generic_impl_method_template(typed, def_id) {
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
        lang_item: typed
            .lang_items
            .marker_for_def(def_id)
            .map(|m| super::format::PxiLangItem {
                name: m.name,
                kind: m.kind.as_str().to_owned(),
            }),
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
        DefKind::Fn | DefKind::ImplMethod => {
            typed.functions.iter().find(|f| f.def == def_id).map(|f| {
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
            })
        }
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
            path: format!(
                "{logical_module}::{}",
                interner.resolve(def.name).unwrap_or("<?>")
            ),
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
        DefKind::Fn | DefKind::ImplMethod => typed
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
        DefKind::Struct | DefKind::Enum | DefKind::TypeAlias => names.resolve_display(def.name),
        _ => def_kind_to_pxi(def.kind).to_owned(),
    }
}

/// Collects direct `#import` dependencies with current `.pxi` content digests.
///
/// Walks imports in `module`, canonicalizes each target against `workspace_name` and
/// `dep_names`, and reads the dependency's on-disk `.pxi` at `layout` artifact paths.
/// Duplicate imports and unresolved targets are skipped. Missing or unreadable `.pxi`
/// files produce an empty `pxi_hash` (forcing rebuild when the dependency is compiled).
#[must_use]
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

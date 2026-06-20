//! Global function-id map and dependency link inputs (M2 driver).
//!
//! Bridges cross-crate codegen and the linker by assigning stable numeric function
//! indices to every [`DefId`] that appears in IR or dependency exports. Called from
//! [`super::package::build_package`] after type-check and before per-module codegen.
//!
//! ## Responsibilities
//!
//! - **Global map** — [`build_global_fn_map`] merges dependency `.pxi` `function_id`
//!   values with freshly allocated ids for workspace-local functions and in-crate
//!   monomorphizations of dependency generics.
//! - **Link inputs** — [`append_dependency_link_inputs`] decodes prebuilt dependency
//!   `.phx0` objects from each path dependency's manifest for the final link step.
//! - **Interface stability** — [`verify_pxi_exports`] rejects signature changes on
//!   existing exports when regenerating `.pxi` files.
//!
//! ## Inputs and outputs
//!
//! | Function | Reads | Produces |
//! | --- | --- | --- |
//! | [`build_global_fn_map`] | [`ProjectConfig`], [`LoadedProgram`], [`TypedProgram`], IR | `HashMap<DefId, u32>` for codegen |
//! | [`append_dependency_link_inputs`] | dependency manifests under `build/deps/` | extra [`LinkInput`] slices |
//! | [`collect_export_maps`] | [`ResolvedProgram`](crate::resolver::ResolvedProgram) | per-module export name → [`DefId`] |
//! | [`verify_pxi_exports`] | old manifest + new [`PxiFile`] | `Ok(())` or [`BuildError::InterfaceMismatch`] |
//!
//! Dependency `.pxi` files must list every imported function export with a `function_id`
//! (except template/generic exports resolved at monomorphization time). A stale or
//! incomplete dependency interface surfaces as [`BuildError::StaleInterface`].

use std::collections::{HashMap, HashSet};
use std::path::Path;

use phx_bytecode::BytecodeModule;

use crate::link::LinkInput;
use crate::modules::{LoadedProgram, ProgramLoadContext};
use crate::project::{BuildLayout, ProjectConfig};
use crate::pxi::{PxiFile, stable_export_id};
use crate::resolver::DefId;
use crate::typeck::{CrossCrateMonoReq, mangle_export_id};

use super::super::error::BuildError;
use super::super::manifest::{BuildManifest, resolve_manifest_path};
use super::util::module_in_workspace_package;

/// Returns `true` when a path dependency's `.pxi` already exports a monomorphized symbol.
///
/// Looks up the dependency crate's module artifact for `req.logical_module`, reads its
/// `.pxi`, and checks for a function export whose name matches [`CrossCrateMonoReq::mangled_name`]
/// with a populated `function_id`. Used by the driver to decide whether cross-crate
/// monomorphization work can be satisfied from a prebuilt dependency or must be lowered
/// in the consumer crate.
pub(super) fn dep_pxi_has_mangled_export(
    consumer: &ProjectConfig,
    dep_cfg: &ProjectConfig,
    req: &CrossCrateMonoReq,
) -> bool {
    let dep_layout = BuildLayout::for_dependency(consumer, &dep_cfg.name);
    let pxi_path = dep_layout.module_artifacts(&req.logical_module).pxi;
    let Ok(pxi) = PxiFile::read_from_path(&pxi_path) else {
        return false;
    };
    pxi.exports
        .iter()
        .any(|e| e.name == req.mangled_name && e.kind == "fn" && e.function_id.is_some())
}

/// Builds per-module export name → [`DefId`] maps from the resolved program.
///
/// Iterates [`ResolvedProgram::defs`](crate::resolver::ResolvedProgram) and records
/// each exported definition in the vector slot indexed by its owning module id. The
/// resulting table is indexed by module id during
/// [`super::artifacts::write_interfaces_and_collect_objects`] when constructing
/// `.pxi` export lists.
pub(super) fn collect_export_maps(
    resolved: &crate::resolver::ResolvedProgram,
) -> Vec<HashMap<phx_syntax::Symbol, DefId>> {
    let mut per_module: Vec<HashMap<phx_syntax::Symbol, DefId>> =
        vec![HashMap::new(); resolved.modules.len()];
    for (i, def) in resolved.defs.iter().enumerate() {
        if def.exported {
            let idx = def.module as usize;
            if idx < per_module.len() {
                per_module[idx].insert(def.name, DefId::from_raw(u32::try_from(i).unwrap_or(0)));
            }
        }
    }
    per_module
}

/// Appends decoded dependency `.phx0` modules to the workspace link input list.
///
/// For each path dependency in `config`, loads its [`BuildManifest`] from
/// `build/deps/{name}/manifest.json`, resolves each recorded `phx0_path` relative to
/// that dependency's build root, decodes the bytecode, and pushes a [`LinkInput`].
/// Workspace-local modules are not included here — they are collected separately by
/// [`super::artifacts::write_interfaces_and_collect_objects`].
///
/// # Errors
///
/// Returns [`BuildError::StaleInterface`] when a dependency manifest is missing.
/// Returns [`BuildError::Io`] when a `.phx0` file cannot be read or decoded.
pub(super) fn append_dependency_link_inputs(
    config: &ProjectConfig,
    link_inputs: &mut Vec<LinkInput>,
) -> Result<(), BuildError> {
    for dep in config.dependencies.values() {
        let dep_root = config.root.join(&dep.path);
        let dep_cfg = ProjectConfig::load(&dep_root).map_err(BuildError::Project)?;
        let dep_layout = BuildLayout::for_dependency(config, &dep_cfg.name);
        let dep_manifest_path = dep_layout.manifest_path();
        let Some(manifest) = BuildManifest::read(&dep_manifest_path) else {
            return Err(BuildError::StaleInterface {
                module: dep_cfg.name.clone(),
                message: format!(
                    "missing dependency manifest at {}",
                    dep_manifest_path.display()
                ),
            });
        };
        for rec in manifest.modules.values() {
            let phx0_path = resolve_manifest_path(dep_layout.build_root(), &rec.phx0_path);
            let bytes = std::fs::read(&phx0_path).map_err(|e| BuildError::Io {
                path: phx0_path.clone(),
                message: e.to_string(),
            })?;
            let module = BytecodeModule::decode(&bytes).map_err(|e| BuildError::Io {
                path: phx0_path,
                message: format!("{e:?}"),
            })?;
            link_inputs.push(LinkInput {
                logical_path: rec.logical_path.clone(),
                module,
            });
        }
    }
    Ok(())
}

/// Builds the global [`DefId`] → function index map for codegen and linking.
///
/// Produces the table passed to [`codegen_module`](crate::codegen::codegen_module) so
/// call sites across crates agree on numeric function ids embedded in bytecode.
///
/// ## Algorithm
///
/// 1. Scan every path dependency manifest and collect `function_id` values from `.pxi`
///    fn exports into `dep_export_fn_ids`; track generic/template exports separately.
/// 2. For defs in dependency logical modules, map imported functions to the dependency's
///    stable or mangled export id (skipping template exports monomorphized locally).
/// 3. Walk IR functions: reuse dependency ids where already mapped; allocate sequential
///    ids starting at `max(dep ids) + 1` for workspace-local defs and in-crate mono
///    instances of dependency generic impl methods.
///
/// When `config.dependencies` is empty, ids start at `0` and increment per IR function
/// in workspace modules only.
///
/// # Errors
///
/// Returns [`BuildError::StaleInterface`] when an imported function lacks a matching
/// `function_id` in the dependency `.pxi`, or when a dependency manifest is missing.
/// Returns [`BuildError::Project`] / [`BuildError::Pxi`] when dependency configuration
/// or interface files cannot be loaded.
#[allow(clippy::too_many_lines)]
pub(super) fn build_global_fn_map(
    config: &ProjectConfig,
    _layout: &BuildLayout,
    _load_ctx: &ProgramLoadContext,
    loaded: &LoadedProgram,
    typed: &crate::typeck::TypedProgram,
    ir: &crate::ir::IrModule,
) -> Result<HashMap<DefId, u32>, BuildError> {
    let workspace = &config.name;
    let interner = &typed.resolved.interner;

    let mut dep_export_fn_ids: HashMap<String, u32> = HashMap::new();
    let mut dep_template_fn_exports: HashSet<String> = HashSet::new();
    let mut max_dep_id = 0u32;

    for dep in config.dependencies.values() {
        let dep_root = config.root.join(&dep.path);
        let dep_cfg = ProjectConfig::load(&dep_root).map_err(BuildError::Project)?;
        let dep_layout = BuildLayout::for_dependency(config, &dep_cfg.name);
        let manifest_path = dep_layout.manifest_path();
        let manifest =
            BuildManifest::read(&manifest_path).ok_or_else(|| BuildError::StaleInterface {
                module: dep_cfg.name.clone(),
                message: format!("missing dependency manifest at {}", manifest_path.display()),
            })?;
        for rec in manifest.modules.values() {
            let pxi_path = resolve_manifest_path(dep_layout.build_root(), &rec.pxi_path);
            let pxi = PxiFile::read_from_path(&pxi_path).map_err(BuildError::Pxi)?;
            for exp in &pxi.exports {
                if exp.kind != "fn" {
                    continue;
                }
                match exp.function_id {
                    Some(id) => {
                        dep_export_fn_ids.insert(exp.export_id.clone(), id);
                        max_dep_id = max_dep_id.max(id);
                    }
                    None => {
                        dep_template_fn_exports.insert(exp.export_id.clone());
                    }
                }
            }
        }
    }

    let module_logical = |module_id: u32| -> Option<String> {
        loaded
            .modules
            .get(module_id as usize)
            .map(|m| m.logical_path.display())
    };

    let mut map = HashMap::new();

    if !config.dependencies.is_empty() {
        for (i, def) in typed.resolved.defs.iter().enumerate() {
            if !def.kind.is_function_body() {
                continue;
            }
            let def_id = DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX));
            let Some(logical) = module_logical(def.module) else {
                continue;
            };
            if module_in_workspace_package(&logical, workspace) {
                continue;
            }
            // Monomorphized impl methods are lowered in the consumer crate; imported generic
            // free functions map to the dependency's mangled export.
            if typed.specialized_from.contains_key(&def_id) {
                let name = interner.resolve(def.name).unwrap_or("<?>");
                let export_id = mangle_export_id(&logical, "fn", name);
                if let Some(&id) = dep_export_fn_ids.get(&export_id) {
                    map.insert(def_id, id);
                }
                continue;
            }
            let name = interner.resolve(def.name).unwrap_or("<?>");
            let export_id = stable_export_id(&logical, name, "fn");
            if dep_template_fn_exports.contains(&export_id) {
                continue;
            }
            let id =
                dep_export_fn_ids
                    .get(&export_id)
                    .ok_or_else(|| BuildError::StaleInterface {
                        module: logical.clone(),
                        message: format!(
                            "missing function_id for export `{export_id}` in dependency `.pxi`"
                        ),
                    })?;
            map.insert(def_id, *id);
        }
    }

    let mut next = if config.dependencies.is_empty() {
        0
    } else {
        max_dep_id.saturating_add(1)
    };

    for f in &ir.functions {
        let Some(def) = typed.resolved.defs.get(f.def.index() as usize) else {
            continue;
        };
        let Some(logical) = module_logical(def.module) else {
            continue;
        };
        if !module_in_workspace_package(&logical, workspace) {
            if map.contains_key(&f.def) {
                continue;
            }
            let name = interner.resolve(def.name).unwrap_or("<?>");
            let export_id = if typed.specialized_from.contains_key(&f.def) {
                mangle_export_id(&logical, "fn", name)
            } else {
                stable_export_id(&logical, name, "fn")
            };
            if dep_template_fn_exports.contains(&export_id) {
                continue;
            }
            if let Some(&id) = dep_export_fn_ids.get(&export_id) {
                map.insert(f.def, id);
                continue;
            }
            // Dependency generic impl method monomorphized in this crate.
            map.insert(f.def, next);
            next = next.saturating_add(1);
            continue;
        }
        if map.contains_key(&f.def) {
            continue;
        }
        map.insert(f.def, next);
        next = next.saturating_add(1);
    }

    Ok(map)
}

/// Verifies that regenerated `.pxi` exports preserve signatures of existing exports.
///
/// Compares `new_pxi` against the on-disk interface recorded in `old` for `logical`.
/// Exports present in both files must have identical `signature` strings; new exports
/// and removed exports are allowed. When the previous `.pxi` file is missing (stale
/// manifest entry), verification succeeds so the driver can regenerate artifacts.
///
/// # Errors
///
/// Returns [`BuildError::InterfaceMismatch`] when an export name exists in both the old
/// and new interfaces but the signature string differs.
pub(super) fn verify_pxi_exports(
    old: &BuildManifest,
    build_root: &Path,
    logical: &str,
    new_pxi: &PxiFile,
) -> Result<(), BuildError> {
    let Some(old_rec) = old.modules.get(logical) else {
        return Ok(());
    };
    let old_pxi_path = resolve_manifest_path(build_root, &old_rec.pxi_path);
    let Ok(old_pxi) = PxiFile::read_from_path(&old_pxi_path) else {
        // Stale manifest entry with a missing `.pxi` on disk — allow regeneration.
        return Ok(());
    };
    for exp in &new_pxi.exports {
        let Some(old_exp) = old_pxi.exports.iter().find(|e| e.name == exp.name) else {
            continue;
        };
        if old_exp.signature != exp.signature {
            return Err(BuildError::InterfaceMismatch {
                module: logical.to_owned(),
                name: exp.name.clone(),
                message: format!("was `{}`, now `{}`", old_exp.signature, exp.signature),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use crate::build::manifest::{BuildManifest, ManifestModule};
    use crate::pxi::PxiFile;

    use super::verify_pxi_exports;

    #[test]
    fn verify_pxi_exports_tolerates_missing_old_interface_file() {
        let mut modules = HashMap::new();
        modules.insert(
            "std::core::copyable".to_owned(),
            ManifestModule {
                logical_path: "std::core::copyable".to_owned(),
                source: "src/core/copyable.phx".to_owned(),
                source_hash: "abc".to_owned(),
                pxi_hash: "def".to_owned(),
                phx0_path: "phx0/std/core/copyable.phx0".to_owned(),
                pxi_path: "pxi/std/core/copyable.pxi".to_owned(),
            },
        );
        let old = BuildManifest {
            entry: "std".to_owned(),
            bin_path: "lib/std.phx0".to_owned(),
            profile: "dev".to_owned(),
            modules,
        };
        let new_pxi = PxiFile::parse(
            r#"{
  "format_version": 1,
  "logical_module": "std::core::copyable",
  "source_path": "src/core/copyable.phx",
  "source_hash": "abc",
  "exports": [],
  "dependencies": []
}"#,
        )
        .expect("parse new pxi");
        verify_pxi_exports(
            &old,
            Path::new("/nonexistent/build"),
            "std::core::copyable",
            &new_pxi,
        )
        .expect("missing old .pxi should not block regeneration");
    }
}

//! Global function-id map and dependency link inputs.

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

/// Returns true when the dependency `.pxi` exports the mangled mono symbol.
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

/// Per-module export maps from the resolved program.
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

/// Appends decoded dependency `.phx0` modules to the link input list.
///
/// # Errors
///
/// Returns [`BuildError`] when a dependency manifest or object file is missing.
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

/// Builds the global `DefId` → function index map for codegen and linking.
///
/// # Errors
///
/// Returns [`BuildError`] when dependency `.pxi` files are stale or incomplete.
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

/// Verifies that new `.pxi` exports do not change signatures of existing exports.
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
    let old_pxi = PxiFile::read_from_path(&old_pxi_path).map_err(BuildError::Pxi)?;
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

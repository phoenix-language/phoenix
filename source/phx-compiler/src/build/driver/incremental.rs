//! Incremental freshness checks for workspace and path-dependency builds (M2).
//!
//! Compares live source and `.pxi` digests against `build/manifest.json` to decide
//! whether a module, workspace, or prebuilt dependency crate can be reused without
//! recompilation. Drives skip paths in the artifact emitter and early-return in
//! [`super::package::build_project`].

use std::collections::HashSet;

use crate::modules::{LoadedProgram, ProgramLoadContext, load_program_with_context};
use crate::project::{BuildLayout, ProjectConfig};
use crate::pxi::{PxiDependency, PxiFile, digest_file, module_dependencies};

use super::super::manifest::{BuildManifest, module_is_up_to_date, resolve_manifest_path};
use super::super::options::BuildOptions;
use super::util::module_in_workspace_package;

/// Returns true when any import dependency is in the stale set.
pub(super) fn module_imports_stale(deps: &[PxiDependency], stale: &HashSet<String>) -> bool {
    deps.iter().any(|d| stale.contains(&d.logical_module))
}

/// Workspace modules whose source or dependency hashes differ from the manifest.
pub(super) fn workspace_stale_modules(
    loaded: &LoadedProgram,
    layout: &BuildLayout,
    dep_names: &[&str],
    old_manifest: Option<&BuildManifest>,
    options: BuildOptions,
) -> HashSet<String> {
    let Some(old) = old_manifest else {
        return HashSet::new();
    };
    if options.force {
        return HashSet::new();
    }
    loaded
        .modules
        .iter()
        .filter(|m| module_in_workspace_package(&m.logical_path.display(), &loaded.package_name))
        .filter(|m| {
            let logical = m.logical_path.display();
            let source_hash = digest_file(&m.filesystem).unwrap_or_default();
            let deps = module_dependencies(
                m,
                layout,
                &loaded.path_index,
                &loaded.interner,
                &loaded.package_name,
                dep_names,
            );
            let dep_hashes: Vec<_> = deps
                .iter()
                .map(|d| (d.logical_module.clone(), d.pxi_hash.clone()))
                .collect();
            !module_is_up_to_date(
                old,
                layout.build_root(),
                &logical,
                &source_hash,
                &dep_hashes,
            )
        })
        .map(|m| m.logical_path.display())
        .collect()
}

/// Returns true when every workspace module matches the manifest.
pub(super) fn all_modules_fresh(
    manifest: &BuildManifest,
    loaded: &LoadedProgram,
    layout: &BuildLayout,
    ctx: &ProgramLoadContext,
) -> bool {
    let dep_names: Vec<&str> = ctx.dep_names();
    loaded
        .modules
        .iter()
        .filter(|m| module_in_workspace_package(&m.logical_path.display(), &loaded.package_name))
        .all(|m| {
            let logical = m.logical_path.display();
            let source_hash = digest_file(&m.filesystem).unwrap_or_default();
            let deps = module_dependencies(
                m,
                layout,
                &loaded.path_index,
                &loaded.interner,
                &loaded.package_name,
                &dep_names,
            );
            let dep_hashes: Vec<_> = deps
                .iter()
                .map(|d| (d.logical_module.clone(), d.pxi_hash.clone()))
                .collect();
            module_is_up_to_date(
                manifest,
                layout.build_root(),
                &logical,
                &source_hash,
                &dep_hashes,
            )
        })
}

/// Returns false when any fn export in dependency `.pxi` files lacks `function_id`.
pub(super) fn dependency_pxi_has_function_ids(
    manifest: &BuildManifest,
    build_root: &std::path::Path,
) -> bool {
    for rec in manifest.modules.values() {
        let pxi_path = resolve_manifest_path(build_root, &rec.pxi_path);
        let Ok(pxi) = PxiFile::read_from_path(&pxi_path) else {
            return false;
        };
        for exp in &pxi.exports {
            if exp.kind == "fn" && exp.name.contains('$') && exp.function_id.is_none() {
                return false;
            }
        }
    }
    true
}

/// Returns true when a dependency crate's linked output and manifest are up to date.
pub(super) fn dependency_build_is_fresh(
    dep_cfg: &ProjectConfig,
    dep_layout: &BuildLayout,
    options: BuildOptions,
) -> bool {
    if options.force {
        return false;
    }
    let out = dep_layout.lib_path(&dep_cfg.name);
    if !out.is_file() {
        return false;
    }
    let manifest_path = dep_layout.manifest_path();
    let Some(manifest) = BuildManifest::read(&manifest_path) else {
        return false;
    };
    if !dependency_pxi_has_function_ids(&manifest, dep_layout.build_root()) {
        return false;
    }
    let entry = dep_cfg.default_entry_file();
    let ctx = ProgramLoadContext::from_config(dep_cfg);
    let mut bag = phx_diagnostics::DiagnosticBag::new();
    let Some(loaded) = load_program_with_context(&entry, &ctx, Some(dep_layout), &mut bag) else {
        return false;
    };
    if bag.has_errors() {
        return false;
    }
    all_modules_fresh(&manifest, &loaded, dep_layout, &ctx)
}

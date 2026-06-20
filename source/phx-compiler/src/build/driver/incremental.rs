//! Incremental freshness checks for workspace and path-dependency builds (M2 driver).
//!
//! Compares live source digests and dependency `.pxi` hashes against the recorded
//! [`BuildManifest`] to decide whether modules, the whole workspace, or a prebuilt
//! path dependency can be reused without recompilation. Drives skip paths in
//! [`super::artifacts::write_interfaces_and_collect_objects`] and early returns in
//! [`super::package::build_project`].
//!
//! ## Staleness model
//!
//! A workspace module is **fresh** when all of the following hold:
//!
//! - Its source file digest matches the manifest entry.
//! - Every import dependency's `(logical_module, pxi_hash)` pair matches the manifest.
//! - No transitive import depends on a module in the stale set
//!   ([`module_imports_stale`]).
//!
//! A path dependency crate is **fresh** when its linked `lib/*.phx0` exists, its
//! manifest loads, every mangled fn export in dependency `.pxi` files has a
//! `function_id`, and [`all_modules_fresh`] succeeds for the dependency's own sources.
//!
//! [`BuildOptions::force`] bypasses freshness and always triggers a full rebuild.
//!
//! ## Submodule map
//!
//! | Function | Role |
//! | --- | --- |
//! | [`workspace_stale_modules`] | Collect workspace logical paths needing rebuild |
//! | [`all_modules_fresh`] | True when every workspace module matches manifest |
//! | [`dependency_build_is_fresh`] | True when a path dependency crate can be skipped |
//! | [`dependency_pxi_has_function_ids`] | Guard for legacy manifests missing ids |
//! | [`module_imports_stale`] | Transitive invalidation via import graph |

use std::collections::HashSet;

use crate::modules::{LoadedProgram, ProgramLoadContext, load_program_with_context};
use crate::project::{BuildLayout, ProjectConfig};
use crate::pxi::{PxiDependency, PxiFile, digest_file, module_dependencies};

use super::super::manifest::{BuildManifest, module_is_up_to_date, resolve_manifest_path};
use super::super::options::BuildOptions;
use super::util::module_in_workspace_package;

/// Returns `true` when any direct import dependency is in the stale module set.
///
/// Used during per-module artifact emission to invalidate a module whose own source
/// hash is unchanged but that imports a logical path listed in `stale` (typically
/// from [`workspace_stale_modules`]).
pub(super) fn module_imports_stale(deps: &[PxiDependency], stale: &HashSet<String>) -> bool {
    deps.iter().any(|d| stale.contains(&d.logical_module))
}

/// Returns workspace logical module paths whose artifacts are out of date.
///
/// Filters `loaded.modules` to those owned by the workspace package (see
/// [`super::util::module_in_workspace_package`]) and compares each module's source
/// digest and dependency hash list against `old_manifest` via
/// [`module_is_up_to_date`](crate::build::manifest::module_is_up_to_date).
///
/// Returns an empty set when `old_manifest` is `None` (first build) or when
/// [`BuildOptions::force`] is set — callers treat a non-empty set as "rebuild all
/// workspace modules" to propagate transitive changes.
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

/// Returns `true` when every workspace module matches the manifest.
///
/// Recomputes source digests and import dependency hashes for each workspace-owned
/// module in `loaded` and requires each to pass
/// [`module_is_up_to_date`](crate::build::manifest::module_is_up_to_date) against
/// `manifest`. Used by [`dependency_build_is_fresh`] after reloading a dependency
/// program to confirm its prebuilt artifacts still match live sources.
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

/// Returns `false` when any mangled fn export in dependency `.pxi` files lacks `function_id`.
///
/// Scans every module record in `manifest`, reads the corresponding `.pxi` under
/// `build_root`, and rejects manifests where a function export name contains `$`
/// (mangled monomorphization symbol) but has no assigned `function_id`. This guards
/// against incremental cache hits on dependency builds produced before cross-crate
/// function-id assignment was enforced.
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

/// Returns `true` when a path dependency's linked output and manifest are up to date.
///
/// Checks, in order:
///
/// 1. [`BuildOptions::force`] is not set.
/// 2. The dependency's linked library file (`build/deps/{name}/lib/{name}.phx0`) exists.
/// 3. The dependency manifest loads from disk.
/// 4. [`dependency_pxi_has_function_ids`] passes for that manifest.
/// 5. The dependency entry program reloads without diagnostics errors.
/// 6. [`all_modules_fresh`] succeeds for the reloaded program.
///
/// When this returns `true`, [`super::package::build_dependency`] can skip rebuilding
/// the dependency crate.
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

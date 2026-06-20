//! `.pxi` / `.phx0` artifact emission and manifest recording.

use std::collections::HashMap;
use std::path::Path;

use phx_bytecode::BytecodeModule;

use crate::codegen::codegen_module;
use crate::link::LinkInput;
use crate::lower::lower_module;
use crate::modules::LoadedProgram;
use crate::project::{BuildLayout, ProjectConfig};
use crate::pxi::{build_pxi_for_module, digest_file, module_dependencies};
use crate::resolver::DefId;

use super::super::error::BuildError;
use super::super::manifest::{
    BuildManifest, ManifestModule, module_is_up_to_date, record_pxi_hash, store_path_relative_to,
};
use super::super::options::BuildOptions;
use super::incremental::{module_imports_stale, workspace_stale_modules};
use super::link_map::{collect_export_maps, verify_pxi_exports};
use super::util::{io_err, io_err_path, module_in_workspace_package};

/// Inputs shared by interface emission and incremental object collection.
pub(super) struct ArtifactEmitCtx<'a> {
    pub config: &'a ProjectConfig,
    pub loaded: &'a LoadedProgram,
    pub typed: &'a crate::typeck::TypedProgram,
    pub layout: &'a BuildLayout,
    pub bin_path: &'a str,
    pub entry_logical: &'a str,
    pub options: BuildOptions,
    pub old_manifest: Option<&'a BuildManifest>,
    pub global_fn: &'a HashMap<DefId, u32>,
}

/// Writes `.pxi` files and updates `manifest.json` without linking.
///
/// # Errors
///
/// Returns [`BuildError`] on I/O or interface verification failure.
pub(super) fn write_interfaces_and_manifest(
    config: &ProjectConfig,
    loaded: &LoadedProgram,
    typed: &crate::typeck::TypedProgram,
    layout: &BuildLayout,
    bin_path: &str,
    entry_logical: &str,
    options: BuildOptions,
) -> Result<BuildManifest, BuildError> {
    let old_manifest = BuildManifest::read(&layout.manifest_path());
    let ctx = ArtifactEmitCtx {
        config,
        loaded,
        typed,
        layout,
        bin_path,
        entry_logical,
        options,
        old_manifest: old_manifest.as_ref(),
        global_fn: &HashMap::new(),
    };
    let (_, manifest) = write_interfaces_and_collect_objects(&ctx)?;
    manifest
        .write(&layout.manifest_path())
        .map_err(|e| io_err(&e))?;
    Ok(manifest)
}

/// Emits per-module `.pxi` and optional `.phx0` artifacts; returns link inputs and manifest.
///
/// # Errors
///
/// Returns [`BuildError`] on lower, codegen, encode, or I/O failure.
#[allow(clippy::too_many_lines)] // per-module pxi + optional phx0 loop
pub(super) fn write_interfaces_and_collect_objects(
    ctx: &ArtifactEmitCtx<'_>,
) -> Result<(Vec<LinkInput>, BuildManifest), BuildError> {
    let ArtifactEmitCtx {
        config,
        loaded,
        typed,
        layout,
        bin_path,
        entry_logical,
        options,
        old_manifest,
        global_fn,
    } = ctx;
    let export_maps = collect_export_maps(&typed.resolved);
    let load_ctx = crate::modules::ProgramLoadContext::from_config(config);
    let dep_names: Vec<&str> = load_ctx.dep_names();
    let mut link_inputs = Vec::new();
    let mut manifest = BuildManifest {
        entry: entry_logical.to_string(),
        bin_path: bin_path.to_string(),
        modules: HashMap::new(),
    };

    let stale_modules =
        workspace_stale_modules(loaded, layout, &dep_names, *old_manifest, *options);
    let rebuild_all_workspace = !stale_modules.is_empty();

    for module in &loaded.modules {
        let logical = module.logical_path.display();
        if !module_in_workspace_package(&logical, &loaded.package_name) {
            continue;
        }
        let artifacts = layout.module_artifacts(&logical);
        let source_hash = digest_file(&module.filesystem).unwrap_or_default();
        let exports = &export_maps[module.id.index() as usize];
        let deps = module_dependencies(
            module,
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

        let skip = old_manifest.is_some_and(|old| {
            !options.force
                && !rebuild_all_workspace
                && !stale_modules.contains(&logical)
                && !module_imports_stale(&deps, &stale_modules)
                && module_is_up_to_date(
                    old,
                    layout.build_root(),
                    &logical,
                    &source_hash,
                    &dep_hashes,
                )
        });

        if !skip {
            let pxi = build_pxi_for_module(
                &logical,
                &module.filesystem,
                module.id.index(),
                typed,
                exports,
                &deps,
                Some(global_fn),
            );
            if let Some(old) = old_manifest {
                verify_pxi_exports(old, layout.build_root(), &logical, &pxi)?;
            }
            pxi.write_to_path(&artifacts.pxi)
                .map_err(|e| io_err_path(&artifacts.pxi, &e))?;
        }

        if !options.emit_interface_only {
            let rel_source = module
                .filesystem
                .strip_prefix(&config.root)
                .unwrap_or(&module.filesystem)
                .display()
                .to_string();

            let obj = if skip {
                let phx0_path = old_manifest
                    .and_then(|old| old.modules.get(&logical))
                    .map(|rec| {
                        super::super::manifest::resolve_manifest_path(
                            layout.build_root(),
                            &rec.phx0_path,
                        )
                    })
                    .filter(|p| p.is_file())
                    .unwrap_or_else(|| artifacts.phx0.clone());
                let bytes = std::fs::read(&phx0_path).map_err(|e| io_err_path(&phx0_path, &e))?;
                BytecodeModule::decode(&bytes).map_err(|e| BuildError::Io {
                    path: phx0_path,
                    message: format!("{e:?}"),
                })?
            } else {
                let module_ir =
                    lower_module(typed, module.id.index()).map_err(BuildError::Lower)?;
                let obj = codegen_module(
                    &module_ir,
                    typed,
                    global_fn,
                    module.id == loaded.root,
                    Some(rel_source.as_str()),
                )
                .map_err(BuildError::Codegen)?;
                let bytes = obj.encode().map_err(BuildError::Encode)?;
                if let Some(parent) = artifacts.phx0.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| io_err_path(parent, &e))?;
                }
                std::fs::write(&artifacts.phx0, &bytes)
                    .map_err(|e| io_err_path(&artifacts.phx0, &e))?;
                obj
            };
            link_inputs.push(LinkInput {
                logical_path: logical.clone(),
                module: obj,
            });
        }

        let rel_source = module
            .filesystem
            .strip_prefix(&config.root)
            .unwrap_or(&module.filesystem)
            .display()
            .to_string();

        manifest.modules.insert(
            logical.clone(),
            ManifestModule {
                logical_path: logical,
                source: rel_source,
                source_hash,
                pxi_hash: record_pxi_hash(&artifacts.pxi),
                phx0_path: store_path_relative_to(layout.build_root(), &artifacts.phx0),
                pxi_path: store_path_relative_to(layout.build_root(), &artifacts.pxi),
            },
        );
    }

    Ok((link_inputs, manifest))
}

/// Writes encoded bytecode to `path`, creating parent directories as needed.
pub(super) fn write_module(path: &Path, module: &BytecodeModule) -> Result<(), BuildError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_err(&e))?;
    }
    let bytes = module.encode().map_err(BuildError::Encode)?;
    std::fs::write(path, bytes).map_err(|e| io_err_path(path, &e))
}

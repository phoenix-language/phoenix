//! `phx build` driver — artifacts under `build/`.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use phx_bytecode::{BytecodeModule, ENTRY_NONE};

use crate::codegen::codegen_module;
use crate::compile::CompileError;
use crate::link::{LinkInput, link_modules};
use crate::lower::{lower, lower_module};
use crate::modules::{
    LoadedProgram, ModulePath, ProgramLoadContext, load_program_with_context,
    resolve_loaded_program,
};
use crate::project::{BuildLayout, PackageType, ProjectConfig};
use crate::pxi::{
    PxiFile, build_pxi_for_module, digest_file, module_dependencies, stable_export_id,
};
use crate::resolver::DefId;
use crate::typeck::{
    CrossCrateMonoReq, apply_mono_worklist, collect_cross_crate_mono_reqs, type_check,
};

use super::error::BuildError;
use super::manifest::{
    BuildManifest, ManifestModule, module_is_up_to_date, record_pxi_hash, resolve_manifest_path,
    store_path_relative_to,
};
use super::options::BuildOptions;

/// Inputs shared by interface emission and incremental object collection.
struct ArtifactEmitCtx<'a> {
    config: &'a ProjectConfig,
    loaded: &'a LoadedProgram,
    typed: &'a crate::typeck::TypedProgram,
    layout: &'a BuildLayout,
    bin_path: &'a str,
    entry_logical: &'a str,
    options: BuildOptions,
    old_manifest: Option<&'a BuildManifest>,
    global_fn: &'a HashMap<DefId, u32>,
}

/// Result of a successful project build.
#[derive(Debug, Clone)]
pub struct BuildResult {
    /// Path to linked output (`build/bin/` or `build/lib/`).
    pub output_path: PathBuf,
    /// Entry logical module path.
    pub entry_logical: String,
}

/// Builds the project and writes `build/` artifacts.
///
/// # Errors
///
/// Returns [`BuildError`] on configuration, compile, link, or I/O failure.
pub fn build_project(
    config: &ProjectConfig,
    entry_file: Option<&Path>,
    options: BuildOptions,
) -> Result<BuildResult, BuildError> {
    for dep in config.dependencies.values() {
        build_dependency(config, &config.root.join(&dep.path), options)?;
    }
    build_package(config, entry_file, options, None, None)
}

/// Writes `.pxi` files and `manifest.json` after a successful type-check.
///
/// # Errors
///
/// Returns [`BuildError`] on I/O or interface verification failure.
pub fn emit_interfaces_from_compiled(
    config: &ProjectConfig,
    loaded: &LoadedProgram,
    typed: &crate::typeck::TypedProgram,
    options: BuildOptions,
    layout_override: Option<BuildLayout>,
) -> Result<BuildResult, BuildError> {
    let layout = layout_override.unwrap_or_else(|| BuildLayout::new(config));
    layout
        .ensure_workspace_dirs(config.package_type)
        .map_err(|e| io_err(&e))?;
    let root_module = loaded
        .modules
        .iter()
        .find(|m| m.id == loaded.root)
        .ok_or_else(|| {
            BuildError::Project(crate::project::ProjectError::Invalid {
                message: "missing root module in loaded program".to_owned(),
            })
        })?;
    let entry_logical = entry_logical_path(config, &root_module.filesystem)?;
    let output_path = if options.emit_interface_only {
        layout.manifest_path()
    } else {
        match config.package_type {
            PackageType::Bin => layout.bin_path(config.output_name()),
            PackageType::Lib => layout.lib_path(config.output_name()),
        }
    };
    write_interfaces_and_manifest(
        config,
        loaded,
        typed,
        &layout,
        &output_path.display().to_string(),
        &entry_logical,
        options,
    )?;
    Ok(BuildResult {
        output_path,
        entry_logical,
    })
}

#[allow(clippy::too_many_lines)] // incremental build driver: single orchestration pass
fn build_package(
    config: &ProjectConfig,
    entry_file: Option<&Path>,
    options: BuildOptions,
    layout_override: Option<BuildLayout>,
    injected_mono: Option<&[CrossCrateMonoReq]>,
) -> Result<BuildResult, BuildError> {
    let entry_file = entry_file.map_or_else(|| config.default_entry_file(), Path::to_path_buf);

    let layout = layout_override.unwrap_or_else(|| BuildLayout::new(config));
    layout
        .ensure_workspace_dirs(config.package_type)
        .map_err(|e| io_err(&e))?;

    let ctx = ProgramLoadContext::from_config(config);
    let entry_logical = entry_logical_path(config, &entry_file)?;
    let output_path = match config.package_type {
        PackageType::Bin => layout.bin_path(config.output_name()),
        PackageType::Lib => layout.lib_path(config.output_name()),
    };

    let mut bag = phx_diagnostics::DiagnosticBag::new();
    let loaded = load_program_with_context(&entry_file, &ctx, Some(&layout), &mut bag)
        .ok_or(BuildError::Resolve(bag))?;

    let manifest_path = layout.manifest_path();
    let old_manifest = BuildManifest::read(&manifest_path);

    let artifact_fresh = old_manifest
        .as_ref()
        .is_some_and(|old| all_modules_fresh(old, &loaded, &layout, &ctx));
    let linked_output_fresh = output_path.is_file();
    let needs_full = options.force
        || old_manifest.is_none()
        || !artifact_fresh
        || (!options.emit_interface_only && !linked_output_fresh);

    if !needs_full {
        return Ok(BuildResult {
            output_path,
            entry_logical,
        });
    }

    let resolved = resolve_loaded_program(loaded.clone()).map_err(BuildError::Resolve)?;
    let mut typed = type_check(&resolved).map_err(BuildError::TypeCheck)?;

    let module_logical = |module_id: u32| -> Option<String> {
        loaded
            .modules
            .get(module_id as usize)
            .map(|m| m.logical_path.display())
    };

    if let Some(injected) = injected_mono {
        let bag = apply_mono_worklist(&mut typed, injected, module_logical);
        if bag.has_errors() {
            return Err(BuildError::TypeCheck(bag));
        }
    } else if !config.dependencies.is_empty() {
        reconcile_cross_crate_mono_exports(config, &loaded, &typed, options)?;
    }

    if options.emit_interface_only {
        return emit_interfaces_from_compiled(config, &loaded, &typed, options, Some(layout));
    }

    let full_ir = lower(&typed).map_err(BuildError::Lower)?;
    let load_ctx = ProgramLoadContext::from_config(config);
    let global_fn = build_global_fn_map(config, &layout, &load_ctx, &loaded, &typed, &full_ir)?;
    let ctx = ArtifactEmitCtx {
        config,
        loaded: &loaded,
        typed: &typed,
        layout: &layout,
        bin_path: &output_path.display().to_string(),
        entry_logical: &entry_logical,
        options,
        old_manifest: old_manifest.as_ref(),
        global_fn: &global_fn,
    };
    let (link_inputs, manifest) = write_interfaces_and_collect_objects(&ctx)?;

    let mut link_inputs = link_inputs;
    append_dependency_link_inputs(config, &mut link_inputs)?;

    let entry_fn = match config.package_type {
        PackageType::Bin => typed
            .entry
            .and_then(|d| global_fn.get(&d).copied())
            .ok_or_else(|| {
                let mut bag = phx_diagnostics::DiagnosticBag::new();
                bag.push(
                    0,
                    phx_diagnostics::ResolveError::MissingMain {
                        span: phx_diagnostics::Span::new(0, 1),
                    },
                );
                BuildError::Resolve(bag)
            })?,
        PackageType::Lib => ENTRY_NONE,
    };

    let linked = link_modules(&link_inputs, entry_fn).map_err(BuildError::Link)?;
    phx_bytecode::verify(&linked).map_err(BuildError::Verify)?;
    write_module(&output_path, &linked)?;

    manifest.write(&manifest_path).map_err(|e| io_err(&e))?;

    Ok(BuildResult {
        output_path,
        entry_logical,
    })
}

fn build_dependency(
    consumer: &ProjectConfig,
    dep_root: &Path,
    options: BuildOptions,
) -> Result<(), BuildError> {
    let dep_cfg = ProjectConfig::load(dep_root).map_err(BuildError::Project)?;
    let dep_layout = BuildLayout::for_dependency(consumer, &dep_cfg.name);
    dep_layout.ensure_dep_dirs().map_err(|e| io_err(&e))?;
    if dependency_build_is_fresh(&dep_cfg, &dep_layout, options) {
        return Ok(());
    }
    for nested in dep_cfg.dependencies.values() {
        build_dependency(consumer, &dep_cfg.root.join(&nested.path), options)?;
    }
    build_package(&dep_cfg, None, options, Some(dep_layout), None)?;
    Ok(())
}

fn build_dependency_with_mono_worklist(
    consumer: &ProjectConfig,
    dep_root: &Path,
    worklist: &[CrossCrateMonoReq],
    options: BuildOptions,
) -> Result<(), BuildError> {
    let dep_cfg = ProjectConfig::load(dep_root).map_err(BuildError::Project)?;
    let dep_layout = BuildLayout::for_dependency(consumer, &dep_cfg.name);
    dep_layout.ensure_dep_dirs().map_err(|e| io_err(&e))?;
    for nested in dep_cfg.dependencies.values() {
        build_dependency(consumer, &dep_cfg.root.join(&nested.path), options)?;
    }
    build_package(
        &dep_cfg,
        None,
        BuildOptions {
            force: true,
            ..options
        },
        Some(dep_layout),
        Some(worklist),
    )?;
    Ok(())
}

fn reconcile_cross_crate_mono_exports(
    consumer: &ProjectConfig,
    loaded: &LoadedProgram,
    typed: &crate::typeck::TypedProgram,
    options: BuildOptions,
) -> Result<(), BuildError> {
    let module_logical = |module_id: u32| -> Option<String> {
        loaded
            .modules
            .get(module_id as usize)
            .map(|m| m.logical_path.display())
    };
    let reqs = collect_cross_crate_mono_reqs(typed, &consumer.name, module_logical);
    if reqs.is_empty() {
        return Ok(());
    }
    let mut by_dep: HashMap<String, Vec<CrossCrateMonoReq>> = HashMap::new();
    for req in reqs {
        by_dep.entry(req.dep_package.clone()).or_default().push(req);
    }
    for dep in consumer.dependencies.values() {
        let dep_root = consumer.root.join(&dep.path);
        let dep_cfg = ProjectConfig::load(&dep_root).map_err(BuildError::Project)?;
        let Some(dep_reqs) = by_dep.get(&dep_cfg.name) else {
            continue;
        };
        let missing: Vec<_> = dep_reqs
            .iter()
            .filter(|r| !dep_pxi_has_mangled_export(consumer, &dep_cfg, r))
            .cloned()
            .collect();
        if missing.is_empty() {
            continue;
        }
        build_dependency_with_mono_worklist(consumer, &dep_root, &missing, options)?;
    }
    Ok(())
}

fn dep_pxi_has_mangled_export(
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

fn dependency_build_is_fresh(
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

/// Returns false when any fn export in dependency `.pxi` files lacks `function_id`.
fn dependency_pxi_has_function_ids(manifest: &BuildManifest, build_root: &Path) -> bool {
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

fn write_interfaces_and_manifest(
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

#[allow(clippy::too_many_lines)] // per-module pxi + optional phx0 loop
fn write_interfaces_and_collect_objects(
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
    let load_ctx = ProgramLoadContext::from_config(config);
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
            let obj = if skip {
                let phx0_path = old_manifest
                    .and_then(|old| old.modules.get(&logical))
                    .map(|rec| resolve_manifest_path(layout.build_root(), &rec.phx0_path))
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
                let obj = codegen_module(&module_ir, typed, global_fn, module.id == loaded.root)
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

fn append_dependency_link_inputs(
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

/// Loads the linked binary from `build/bin` for `config`.
///
/// # Errors
///
/// Returns [`BuildError`] when the project was not built or the file is missing.
pub fn load_project_binary(config: &ProjectConfig) -> Result<BytecodeModule, BuildError> {
    if config.package_type != PackageType::Bin {
        return Err(BuildError::Project(crate::project::ProjectError::Invalid {
            message: "phx run requires project.type = bin".to_owned(),
        }));
    }
    let layout = BuildLayout::new(config);
    let bin_path = layout.bin_path(config.output_name());
    let bytes = std::fs::read(&bin_path).map_err(|e| io_err_path(&bin_path, &e))?;
    BytecodeModule::decode(&bytes).map_err(|e| BuildError::Io {
        path: bin_path,
        message: format!("{e:?}"),
    })
}

fn entry_logical_path(config: &ProjectConfig, entry_file: &Path) -> Result<String, BuildError> {
    ModulePath::from_file_path(&config.module_root(), entry_file, &config.name)
        .map(|p| p.display())
        .ok_or_else(|| {
            BuildError::Project(crate::project::ProjectError::Invalid {
                message: "could not derive entry module path from file".to_owned(),
            })
        })
}

fn module_in_workspace_package(logical: &str, workspace: &str) -> bool {
    logical.split("::").next() == Some(workspace)
}

fn module_imports_stale(deps: &[crate::pxi::PxiDependency], stale: &HashSet<String>) -> bool {
    deps.iter().any(|d| stale.contains(&d.logical_module))
}

fn workspace_stale_modules(
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

#[allow(clippy::too_many_lines)]
fn build_global_fn_map(
    config: &ProjectConfig,
    _layout: &BuildLayout,
    _load_ctx: &ProgramLoadContext,
    loaded: &LoadedProgram,
    typed: &crate::typeck::TypedProgram,
    ir: &crate::ir::IrModule,
) -> Result<HashMap<DefId, u32>, BuildError> {
    use crate::resolver::DefKind;

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
            if def.kind != DefKind::Fn {
                continue;
            }
            let Some(logical) = module_logical(def.module) else {
                continue;
            };
            if module_in_workspace_package(&logical, workspace) {
                continue;
            }
            let name = interner.resolve(def.name);
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
            map.insert(DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)), *id);
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
            if !map.contains_key(&f.def) {
                let name = interner.resolve(def.name);
                let export_id = stable_export_id(&logical, name, "fn");
                if dep_template_fn_exports.contains(&export_id) {
                    continue;
                }
                return Err(BuildError::StaleInterface {
                    module: logical,
                    message: format!("missing function_id for dependency fn `{name}` in `.pxi`"),
                });
            }
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

fn collect_export_maps(
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

fn all_modules_fresh(
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

fn verify_pxi_exports(
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

fn write_module(path: &Path, module: &BytecodeModule) -> Result<(), BuildError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_err(&e))?;
    }
    let bytes = module.encode().map_err(BuildError::Encode)?;
    std::fs::write(path, bytes).map_err(|e| io_err_path(path, &e))
}

fn io_err(e: &std::io::Error) -> BuildError {
    BuildError::Io {
        path: PathBuf::new(),
        message: e.to_string(),
    }
}

fn io_err_path(path: &Path, e: &std::io::Error) -> BuildError {
    BuildError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    }
}

impl From<CompileError> for BuildError {
    fn from(e: CompileError) -> Self {
        match e {
            CompileError::Parse(p) => Self::Parse(p),
            CompileError::Resolve { bag, .. } => Self::Resolve(bag),
            CompileError::TypeCheck { bag, .. } => Self::TypeCheck(bag),
            CompileError::Lower { bag, .. } => Self::Lower(bag),
            CompileError::Codegen(e) => Self::Codegen(e),
            CompileError::Io(e) => Self::Io {
                path: PathBuf::new(),
                message: e.to_string(),
            },
        }
    }
}

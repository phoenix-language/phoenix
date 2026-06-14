//! Per-package and dependency build orchestration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use phx_bytecode::{BytecodeModule, ENTRY_NONE};

use crate::compile::{CompileError, DiagnosticContext, lint_checked};
use crate::link::link_modules;
use crate::lower::lower;
use crate::modules::{
    LoadedProgram, ProgramLoadContext, load_program_with_context, resolve_loaded_program,
};
use crate::project::{BuildLayout, PackageType, ProjectConfig};
use crate::typeck::{
    CrossCrateMonoReq, apply_mono_worklist, collect_cross_crate_mono_reqs, type_check,
};

use super::super::error::BuildError;
use super::super::manifest::BuildManifest;
use super::super::options::BuildOptions;
use super::BuildResult;
use super::artifacts::{
    ArtifactEmitCtx, write_interfaces_and_collect_objects, write_interfaces_and_manifest,
    write_module,
};
use super::incremental::{all_modules_fresh, dependency_build_is_fresh};
use super::link_map::{
    append_dependency_link_inputs, build_global_fn_map, dep_pxi_has_mangled_export,
};
use super::util::{entry_logical_path, io_err, io_err_path};

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
        lints: phx_diagnostics::LintBag::new(),
        lint_context: None,
    })
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
            lints: phx_diagnostics::LintBag::new(),
            lint_context: None,
        });
    }

    let resolved = resolve_loaded_program(loaded.clone()).map_err(BuildError::Resolve)?;
    let mut typed = type_check(resolved).map_err(BuildError::TypeCheck)?;

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

    let lint_context = DiagnosticContext::from_resolved(&typed.resolved);
    let lints = lint_checked(&typed).map_err(BuildError::Resolve)?;

    if options.emit_interface_only {
        let mut result =
            emit_interfaces_from_compiled(config, &loaded, &typed, options, Some(layout))?;
        result.lints = lints;
        result.lint_context = Some(lint_context);
        return Ok(result);
    }

    let full_ir = lower(&typed).map_err(BuildError::Lower)?;
    #[cfg(any(debug_assertions, test))]
    crate::ir::validate_ir(&full_ir, &typed).map_err(BuildError::IrValidate)?;
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
        lints,
        lint_context: Some(lint_context),
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

impl From<CompileError> for BuildError {
    fn from(e: CompileError) -> Self {
        match e {
            CompileError::Parse(p) => Self::Parse(p),
            CompileError::Resolve { bag, .. } => Self::Resolve(bag),
            CompileError::TypeCheck { bag, .. } => Self::TypeCheck(bag),
            CompileError::Lower { bag, .. } => Self::Lower(bag),
            CompileError::IrValidate { bag, .. } => Self::IrValidate(bag),
            CompileError::Codegen(e) => Self::Codegen(e),
            CompileError::Io(e) => Self::Io {
                path: PathBuf::new(),
                message: e.to_string(),
            },
        }
    }
}

//! `phx build` driver — artifacts under `build/`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use phx_bytecode::BytecodeModule;

use crate::codegen::codegen_module;
use crate::compile::CompileError;
use crate::link::{LinkInput, link_modules};
use crate::lower::{lower, lower_module};
use crate::modules::{
    CrateLoadContext, LoadedCrate, ModulePath, load_crate_with_context, resolve_crate,
};
use crate::project::{BuildLayout, PackageType, ProjectConfig};
use crate::pxi::{PxiFile, build_pxi_for_module, digest_file, module_dependencies};
use crate::resolver::DefId;
use crate::typeck::type_check;

use super::error::BuildError;
use super::manifest::{BuildManifest, ManifestModule, module_is_up_to_date, record_pxi_hash};

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
    force: bool,
) -> Result<BuildResult, BuildError> {
    for (_key, dep) in &config.dependencies {
        build_dependency(config, &config.root.join(&dep.path), force)?;
    }
    build_package(config, entry_file, force, None)
}

fn build_package(
    config: &ProjectConfig,
    entry_file: Option<&Path>,
    force: bool,
    layout_override: Option<BuildLayout>,
) -> Result<BuildResult, BuildError> {
    let entry_file = entry_file
        .map(Path::to_path_buf)
        .unwrap_or_else(|| config.default_entry_file());

    let layout = layout_override.unwrap_or_else(|| BuildLayout::new(config));
    layout
        .ensure_workspace_dirs(config.package_type)
        .map_err(io_err)?;

    let ctx = CrateLoadContext::from_config(config);
    let entry_logical = entry_logical_path(config, &entry_file)?;
    let output_path = match config.package_type {
        PackageType::Bin => layout.bin_path(config.output_name()),
        PackageType::Lib => layout.lib_path(config.output_name()),
    };

    let mut bag = phx_diagnostics::DiagnosticBag::new();
    let loaded = load_crate_with_context(&entry_file, &ctx, Some(&layout), &mut bag)
        .ok_or(BuildError::Resolve(bag))?;

    let manifest_path = layout.manifest_path();
    let old_manifest = BuildManifest::read(&manifest_path);

    let needs_full = force || !output_path.is_file() || old_manifest.is_none();

    if !needs_full {
        if let Some(ref old) = old_manifest {
            if all_modules_fresh(old, &loaded, &layout, &ctx) {
                return Ok(BuildResult {
                    output_path,
                    entry_logical,
                });
            }
        }
    }

    let resolved = resolve_crate(loaded.clone()).map_err(BuildError::Resolve)?;
    let typed = type_check(&resolved).map_err(BuildError::TypeCheck)?;
    let full_ir = lower(&typed);
    let global_fn = build_global_fn_map(&full_ir);

    let export_maps = collect_export_maps(&resolved);
    let dep_names: Vec<&str> = ctx.dep_names();

    let mut link_inputs = Vec::new();
    let mut manifest = BuildManifest {
        entry: entry_logical.clone(),
        bin_path: output_path.display().to_string(),
        modules: HashMap::new(),
    };

    for module in &loaded.modules {
        let logical = module.logical_path.display();
        let artifacts = layout.module_artifacts(&logical);
        let source_hash = digest_file(&module.filesystem).unwrap_or_default();
        let exports = &export_maps[module.id.index() as usize];
        let deps = module_dependencies(
            module,
            &layout,
            &loaded.path_index,
            &loaded.interner,
            &loaded.package_name,
            &dep_names,
        );
        let dep_hashes: Vec<_> = deps
            .iter()
            .map(|d| (d.logical_module.clone(), d.pxi_hash.clone()))
            .collect();

        let skip = old_manifest.as_ref().is_some_and(|old| {
            !force && module_is_up_to_date(old, &logical, &source_hash, &dep_hashes)
        });

        let pxi = build_pxi_for_module(
            &logical,
            &module.filesystem,
            module.id.index(),
            &typed,
            exports,
            &deps,
        );

        if !skip {
            if let Some(old) = &old_manifest {
                verify_pxi_exports(old, &logical, &pxi)?;
            }
            pxi.write_to_path(&artifacts.pxi)
                .map_err(|e| io_err_path(&artifacts.pxi, e))?;
        }

        let module_ir = lower_module(&typed, module.id.index());
        let obj = codegen_module(&module_ir, &typed, &global_fn, module.id == loaded.root);
        let bytes = obj.encode();
        if let Some(parent) = artifacts.phx0.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io_err_path(parent, e))?;
        }
        std::fs::write(&artifacts.phx0, &bytes).map_err(|e| io_err_path(&artifacts.phx0, e))?;

        link_inputs.push(LinkInput {
            logical_path: logical.clone(),
            module: obj,
        });

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
                phx0_path: artifacts.phx0.display().to_string(),
                pxi_path: artifacts.pxi.display().to_string(),
            },
        );
    }

    append_dependency_link_inputs(config, &mut link_inputs)?;

    let entry_fn = match config.package_type {
        PackageType::Bin => typed
            .entry
            .and_then(|d| global_fn.get(&d).copied())
            .ok_or_else(|| {
                let mut bag = phx_diagnostics::DiagnosticBag::new();
                bag.push(phx_diagnostics::ResolveError::MissingMain);
                BuildError::Resolve(bag)
            })?,
        PackageType::Lib => 0,
    };

    let linked = link_modules(&link_inputs, entry_fn).map_err(BuildError::Link)?;
    write_module(&output_path, &linked)?;

    manifest.write(&manifest_path).map_err(io_err)?;

    Ok(BuildResult {
        output_path,
        entry_logical,
    })
}

fn build_dependency(
    consumer: &ProjectConfig,
    dep_root: &Path,
    force: bool,
) -> Result<(), BuildError> {
    let dep_cfg = ProjectConfig::load(dep_root).map_err(BuildError::Project)?;
    let dep_layout = BuildLayout::for_dependency(consumer, &dep_cfg.name);
    dep_layout.ensure_dep_dirs().map_err(io_err)?;
    let out = dep_layout.lib_path(&dep_cfg.name);
    if out.is_file() && !force {
        return Ok(());
    }
    for (_key, nested) in &dep_cfg.dependencies {
        build_dependency(consumer, &dep_cfg.root.join(&nested.path), force)?;
    }
    build_package(&dep_cfg, None, force, Some(dep_layout))?;
    Ok(())
}

fn append_dependency_link_inputs(
    config: &ProjectConfig,
    link_inputs: &mut Vec<LinkInput>,
) -> Result<(), BuildError> {
    for (_key, dep) in &config.dependencies {
        let dep_root = config.root.join(&dep.path);
        let dep_cfg = ProjectConfig::load(&dep_root).map_err(BuildError::Project)?;
        let dep_layout = BuildLayout::for_dependency(config, &dep_cfg.name);
        let dep_manifest_path = dep_layout.manifest_path();
        let Some(manifest) = BuildManifest::read(&dep_manifest_path) else {
            continue;
        };
        for rec in manifest.modules.values() {
            let bytes = std::fs::read(&rec.phx0_path).map_err(|e| BuildError::Io {
                path: PathBuf::from(&rec.phx0_path),
                message: e.to_string(),
            })?;
            let module = BytecodeModule::decode(&bytes).map_err(|e| BuildError::Io {
                path: PathBuf::from(&rec.phx0_path),
                message: format!("{e:?}"),
            })?;
            if link_inputs
                .iter()
                .any(|i| i.logical_path == rec.logical_path)
            {
                continue;
            }
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
    let bytes = std::fs::read(&bin_path).map_err(|e| io_err_path(&bin_path, e))?;
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

fn build_global_fn_map(ir: &crate::ir::IrModule) -> HashMap<crate::resolver::DefId, u32> {
    ir.functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.def, u32::try_from(i).unwrap_or(u32::MAX)))
        .collect()
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
    loaded: &LoadedCrate,
    layout: &BuildLayout,
    ctx: &CrateLoadContext,
) -> bool {
    let dep_names: Vec<&str> = ctx.dep_names();
    loaded.modules.iter().all(|m| {
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
        module_is_up_to_date(manifest, &logical, &source_hash, &dep_hashes)
    })
}

fn verify_pxi_exports(
    old: &BuildManifest,
    logical: &str,
    new_pxi: &PxiFile,
) -> Result<(), BuildError> {
    let Some(old_rec) = old.modules.get(logical) else {
        return Ok(());
    };
    let old_pxi = PxiFile::read_from_path(Path::new(&old_rec.pxi_path)).map_err(BuildError::Pxi)?;
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
        std::fs::create_dir_all(parent).map_err(io_err)?;
    }
    std::fs::write(path, module.encode()).map_err(|e| io_err_path(path, e))
}

fn io_err(e: std::io::Error) -> BuildError {
    BuildError::Io {
        path: PathBuf::new(),
        message: e.to_string(),
    }
}

fn io_err_path(path: &Path, e: std::io::Error) -> BuildError {
    BuildError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    }
}

impl From<CompileError> for BuildError {
    fn from(e: CompileError) -> Self {
        match e {
            CompileError::Parse(p) => Self::Parse(p),
            CompileError::Resolve(b) => Self::Resolve(b),
            CompileError::TypeCheck(b) => Self::TypeCheck(b),
            CompileError::Io(e) => Self::Io {
                path: PathBuf::new(),
                message: e.to_string(),
            },
        }
    }
}

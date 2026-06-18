//! Project build helpers for embedded and legacy `phoenix.toml` fixtures.

use std::path::Path;

use phx_bytecode::{BytecodeModule, verify};
use phx_compiler::{
    BuildOptions, BuildResult, ProjectConfig, build_project, discover_project, load_project_binary,
};
use phx_programs::ProjectSpec;

use crate::fixtures::{assert_fixture_exists, require_std_project};
use crate::programs::lookup_project;
use crate::sandbox::sandbox_project_root;
use crate::sandbox_lock::{
    project_fs_lock, sandbox_project_name_from_path, std_fs_lock, with_project_fs_lock,
};

/// Result of a forced project build.
#[derive(Debug)]
pub struct BuiltProject {
    /// Discovered project configuration.
    pub config: ProjectConfig,
    /// Build driver result (output paths, manifest, etc.).
    pub result: BuildResult,
    /// Linked bytecode module (verified).
    pub module: BytecodeModule,
    _workspace: Option<()>,
}

fn build_project_spec_inner(
    spec: &ProjectSpec,
    options: BuildOptions,
    remove_build_dir: bool,
) -> BuiltProject {
    let root = sandbox_project_root(spec.name);
    if remove_build_dir {
        rm_project_build_at(&root);
    }
    let config = discover_project(&root).unwrap_or_else(|e| panic!("discover {}: {e}", spec.name));
    let result = build_project(&config, None, options)
        .unwrap_or_else(|e| panic!("build {}: {e}", spec.name));
    let module = load_built_binary(&config).unwrap_or_else(|e| panic!("load {}: {e}", spec.name));
    verify(&module).unwrap_or_else(|e| panic!("verify {}: {e}", spec.name));
    BuiltProject {
        config,
        result,
        module,
        _workspace: None,
    }
}

/// Discover, incrementally build (reusing `build/` when fresh), load, and verify.
pub fn ensure_built_project(name: &str) -> BuiltProject {
    let spec = lookup_project(name);
    let _lock = project_fs_lock(spec.name);
    build_project_spec_inner(spec, BuildOptions::default(), false)
}

/// Like [`ensure_built_project`] without acquiring the project lock (caller must hold it).
pub fn ensure_built_project_unlocked(name: &str) -> BuiltProject {
    build_project_spec_inner(lookup_project(name), BuildOptions::default(), false)
}

/// Discover, force-build, load, and verify an embedded project spec.
pub fn force_build_project_spec(spec: &ProjectSpec) -> BuiltProject {
    let _lock = project_fs_lock(spec.name);
    build_project_spec_inner(spec, BuildOptions::force(true), true)
}

/// Discover, force-build, load, and verify a project by legacy fixture name.
pub fn force_build_project(name: &str) -> BuiltProject {
    force_build_project_spec(lookup_project(name))
}

/// Like [`force_build_project`] without acquiring the project lock (caller must hold it).
pub fn force_build_project_unlocked(name: &str) -> BuiltProject {
    build_project_spec_inner(lookup_project(name), BuildOptions::force(true), true)
}

fn rm_project_build_at(root: &Path) {
    let build = root.join("build");
    if build.exists() {
        std::fs::remove_dir_all(&build).unwrap_or_else(|e| panic!("rm {}: {e}", build.display()));
    }
}

/// Load the linked binary for an already-built project.
///
/// # Errors
///
/// Returns [`phx_compiler::BuildError`] when the project cannot be loaded (e.g. lib packages).
pub fn load_built_binary(
    config: &ProjectConfig,
) -> Result<BytecodeModule, phx_compiler::BuildError> {
    load_project_binary(config)
}

/// Discover project config from a fixture root path.
pub fn discover_cli_project(root: &Path) -> ProjectConfig {
    assert_fixture_exists(root);
    discover_project(root).unwrap_or_else(|e| panic!("discover {}: {e}", root.display()))
}

fn build_cli_project_inner(config: &ProjectConfig, options: BuildOptions) -> BuildResult {
    build_project(config, None, options)
        .unwrap_or_else(|e| panic!("build {}: {e}", config.root.display()))
}

/// Build project at `root` with the given options.
pub fn build_cli_project(config: &ProjectConfig, options: BuildOptions) -> BuildResult {
    if let Some(name) = sandbox_project_name_from_path(&config.root) {
        return with_project_fs_lock(&name, || build_cli_project_inner(config, options));
    }
    build_cli_project_inner(config, options)
}

/// Force-build the repository `std/` project (serialized via [`std_fs_lock`]).
pub fn build_std_project(options: BuildOptions) -> BuildResult {
    let _lock = std_fs_lock();
    let root = require_std_project();
    let config = discover_project(&root).unwrap_or_else(|e| panic!("discover std: {e}"));
    build_project(&config, None, options).unwrap_or_else(|e| panic!("build std: {e}"))
}

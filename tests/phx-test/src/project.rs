//! Project build helpers for embedded and legacy `phoenix.toml` fixtures.

use std::path::Path;

use phx_bytecode::{BytecodeModule, verify};
use phx_compiler::{
    BuildOptions, BuildResult, ProjectConfig, build_project, discover_project, load_project_binary,
};
use phx_programs::ProjectSpec;

use crate::fixtures::assert_fixture_exists;
use crate::programs::lookup_project;
use crate::sandbox::sandbox_project_root;

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

/// Discover, force-build, load, and verify an embedded project spec.
pub fn force_build_project_spec(spec: &ProjectSpec) -> BuiltProject {
    let root = sandbox_project_root(spec.name);
    rm_project_build_at(&root);
    let config = discover_project(&root).unwrap_or_else(|e| panic!("discover {}: {e}", spec.name));
    let result = build_project(&config, None, BuildOptions::force(true))
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

/// Discover, force-build, load, and verify a project by legacy fixture name.
pub fn force_build_project(name: &str) -> BuiltProject {
    force_build_project_spec(lookup_project(name))
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

/// Build project at `root` with the given options.
pub fn build_cli_project(config: &ProjectConfig, options: BuildOptions) -> BuildResult {
    build_project(config, None, options)
        .unwrap_or_else(|e| panic!("build {}: {e}", config.root.display()))
}

//! Project build helpers for `phoenix.toml` fixtures.

use std::path::Path;

use phx_bytecode::{BytecodeModule, verify};
use phx_compiler::{
    BuildOptions, BuildResult, ProjectConfig, build_project, discover_project, load_project_binary,
};

use crate::cli::rm_project_build_unlocked;
use crate::fixtures::{assert_fixture_exists, cli_project};

/// Result of a forced project build.
#[derive(Debug)]
pub struct BuiltProject {
    /// Discovered project configuration.
    pub config: ProjectConfig,
    /// Build driver result (output paths, manifest, etc.).
    pub result: BuildResult,
    /// Linked bytecode module (verified).
    pub module: BytecodeModule,
}

/// Discover, force-build, load, and verify a CLI project fixture by name.
pub fn force_build_project(name: &str) -> BuiltProject {
    let root = cli_project(name);
    assert_fixture_exists(&root);
    rm_project_build_unlocked(name);
    let config = discover_project(&root).unwrap_or_else(|e| panic!("discover {name}: {e}"));
    let result = build_project(&config, None, BuildOptions::force(true))
        .unwrap_or_else(|e| panic!("build {name}: {e}"));
    let module = load_built_binary(&config).unwrap_or_else(|e| panic!("load {name}: {e}"));
    verify(&module).unwrap_or_else(|e| panic!("verify {name}: {e}"));
    BuiltProject {
        config,
        result,
        module,
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

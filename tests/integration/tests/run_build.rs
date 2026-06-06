//! Integration test: project build with phoenix.toml.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_compiler::BuildError;
use phx_compiler::BuildOptions;
use phx_test::{
    build_cli_project, cli_project, discover_cli_project, force_build_project, load_built_binary,
};

#[test]
fn project_build_and_load() {
    let built = force_build_project("project");
    assert!(built.result.output_path.is_file());
    assert!(
        built.module.header.entry_function_id != 0 || !built.module.functions.functions.is_empty()
    );
}

#[test]
fn mvp_acceptance_build_and_load() {
    let built = force_build_project("mvp_acceptance");
    assert!(built.result.output_path.is_file());
    assert!(
        built.module.header.entry_function_id != 0 || !built.module.functions.functions.is_empty()
    );
}

#[test]
fn math_lib_build_produces_lib_artifact() {
    let root = cli_project("math_lib");
    let config = discover_cli_project(&root);
    let result = build_cli_project(&config, BuildOptions::force(true));
    let expected = root.join("build/lib/math.phx0");
    assert_eq!(result.output_path, expected);
    assert!(expected.is_file(), "expected {}", expected.display());
    let err = load_built_binary(&config).expect_err("lib packages are not runnable");
    assert!(
        matches!(err, BuildError::Project(_)),
        "expected project error, got {err:?}"
    );
}

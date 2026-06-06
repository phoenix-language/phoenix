//! Integration test: project build with phoenix.toml.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use phx_compiler::{BuildError, build_project, discover_project, load_project_binary};

#[test]
fn project_build_and_load() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/project");
    let config = discover_project(&root).expect("phoenix.toml");
    let result = build_project(&config, None, true).expect("build");
    assert!(result.output_path.is_file());
    let module = load_project_binary(&config).expect("load");
    assert!(module.header.entry_function_id != 0 || !module.functions.functions.is_empty());
}

#[test]
fn mvp_acceptance_build_and_load() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/mvp_acceptance");
    let config = discover_project(&root).expect("phoenix.toml");
    let result = build_project(&config, None, true).expect("build");
    assert!(result.output_path.is_file());
    let module = load_project_binary(&config).expect("load");
    assert!(module.header.entry_function_id != 0 || !module.functions.functions.is_empty());
}

#[test]
fn math_lib_build_produces_lib_artifact() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/math_lib");
    let config = discover_project(&root).expect("phoenix.toml");
    let result = build_project(&config, None, true).expect("build");
    let expected = root.join("build/lib/math.phx0");
    assert_eq!(result.output_path, expected);
    assert!(expected.is_file(), "expected {}", expected.display());
    let err = load_project_binary(&config).expect_err("lib packages are not runnable");
    assert!(
        matches!(err, BuildError::Project(_)),
        "expected project error, got {err:?}"
    );
}

//! Shared helpers for Phoenix language integration and compiler tests.
//!
//! Centralizes fixture paths, compile/verify/run pipelines, semantic slot assertions,
//! golden diagnostics, project build, incremental fixture patching, and CLI subprocess testing.

#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::missing_panics_doc, clippy::must_use_candidate)]

pub mod cli;
pub mod compile;
pub mod fixtures;
pub mod golden;
pub mod incremental;
pub mod pipeline;
pub mod project;
pub mod semantics;

pub use cli::{
    NEG_CHECK_FIXTURES, PhxCli, PhxOutput, SMOKE_FIXTURES, project_bin_path, rm_project_build,
    rm_project_build_unlocked, shared_cli,
};
pub use compile::{compile_ok, expect_compile_err, expect_resolve_err, expect_typeck_err};
pub use fixtures::{
    assert_fixture_exists, cli_fixture, cli_fixtures_dir, cli_modules_dir, cli_project,
    cli_project_main, examples_dir, examples_project, repo_root, require_cli_project,
    require_fixture_file, require_std_project,
};
pub use golden::{
    assert_golden, format_check_file, format_check_with_module_root, format_compile_source,
    integration_diagnostics_dir, normalize_diagnostics,
};
pub use incremental::{FixturePatch, file_digest, fixture_fs_lock};
pub use pipeline::{
    check_fixture_module_ok, check_fixture_ok, compile_fixture, compile_fixture_module,
    run_fixture_captured, run_fixture_smoke,
};
pub use project::{
    BuiltProject, build_cli_project, discover_cli_project, force_build_project, load_built_binary,
};
pub use semantics::{ExpectedLocal, assert_main_local, assert_main_locals};

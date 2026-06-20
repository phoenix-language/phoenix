//! Shared helpers for Phoenix language integration and compiler tests.
//!
//! Centralizes embedded program materialization, compile/verify/run pipelines,
//! semantic slot assertions, golden diagnostics, project build, and CLI subprocess testing.

#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::missing_panics_doc, clippy::must_use_candidate)]

pub mod cli;
pub mod compile;
pub mod fixtures;
pub mod golden;
pub mod incremental;
pub mod pipeline;
pub mod programs;
pub mod project;
pub mod sandbox;
pub mod sandbox_lock;
pub mod semantics;
pub mod workspace;

pub use cli::{
    NEG_CHECK_FIXTURES, PhxCli, PhxOutput, SMOKE_FIXTURES, phx_bin_path, project_bin_path,
    rm_project_build, shared_cli,
};
pub use compile::{compile_ok, expect_compile_err, expect_resolve_err, expect_typeck_err};
pub use fixtures::{
    assert_fixture_exists, cli_fixture, cli_fixtures_dir, cli_modules_dir, cli_project,
    cli_project_main, examples_dir, examples_project, materialize_module_tree, materialize_project,
    materialize_single, repo_root, require_cli_project, require_fixture_file, require_std_project,
};
pub use golden::{
    assert_golden, assert_golden_expected, format_check_file, format_check_with_module_root,
    format_compile_source, integration_diagnostics_dir, normalize_diagnostics,
};
pub use incremental::{FixturePatch, file_digest};
pub use phx_programs::{
    DIAGNOSTIC_CASES, DiagnosticCase, DiagnosticKind, ModuleTree, NEGATIVE_CASES, ProjectSpec,
    SMOKE_PROGRAMS, SingleFile, SmokeProgram,
};
pub use pipeline::{
    check_fixture_module_ok, check_fixture_ok, check_module_tree_ok, check_program_ok,
    compile_fixture, compile_fixture_module, compile_module_tree, compile_program, compile_smoke,
    project_entry_path, run_fixture_captured, run_fixture_smoke, run_program_captured,
    run_smoke_program,
};
pub use programs::{lookup_module_tree, lookup_project, lookup_single, modules_main_tree};
pub use project::{
    BuiltProject, build_cli_project, build_std_project, discover_cli_project, ensure_built_project,
    ensure_built_project_unlocked, ensure_built_project_with_options, force_build_project,
    force_build_project_spec, force_build_project_unlocked, force_built_project_with_options,
    load_built_binary,
};
pub use sandbox_lock::{project_fs_lock, std_fs_lock, with_project_fs_lock};
pub use semantics::{ExpectedLocal, assert_main_local, assert_main_locals};
pub use workspace::{DEFAULT_RUN_TIMEOUT, TempWorkspace, run_with_timeout};

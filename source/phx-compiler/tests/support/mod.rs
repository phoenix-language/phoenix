//! Local test helpers for phx-compiler pass tests (no phx-test dependency).

#![allow(dead_code)]

use std::path::PathBuf;

use phx_compiler::{CompileError, compile_source};
use phx_diagnostics::{DiagnosticBag, TypeCheckBag};

/// Repository root (`phoenix/`).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// Directory containing CLI test fixtures (`tests/cli/fixtures/`).
pub fn cli_fixtures_dir() -> PathBuf {
    repo_root().join("tests/cli/fixtures")
}

/// Module root for multi-file `#import` fixtures.
pub fn cli_modules_dir() -> PathBuf {
    cli_fixtures_dir().join("modules")
}

/// Expect `compile_source` to succeed.
pub fn compile_ok(source: &str) {
    compile_source(source, None).unwrap_or_else(|e| panic!("expected ok: {e}"));
}

/// Expect a resolve-phase failure and return the diagnostic bag.
pub fn expect_resolve_err(source: &str) -> DiagnosticBag {
    match compile_source(source, None) {
        Err(CompileError::Resolve { bag, .. }) => bag,
        Err(other) => panic!("expected resolve error, got {other}"),
        Ok(_) => panic!("expected resolve error"),
    }
}

/// Expect a type-check failure and return the diagnostic bag.
pub fn expect_typeck_err(source: &str) -> TypeCheckBag {
    match compile_source(source, None) {
        Err(CompileError::TypeCheck { bag, .. }) => bag,
        Err(other) => panic!("expected type-check error, got {other}"),
        Ok(_) => panic!("expected type-check error"),
    }
}

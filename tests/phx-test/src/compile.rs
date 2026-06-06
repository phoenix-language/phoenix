//! Compile-source helpers for compiler pass tests.

use phx_compiler::{CompileError, compile_source};
use phx_diagnostics::{DiagnosticBag, TypeCheckBag};

/// Expect `compile_source` to succeed.
pub fn compile_ok(source: &str) {
    compile_source(source, None).unwrap_or_else(|e| panic!("expected ok: {e}"));
}

/// Expect `compile_source` to fail; run `f` on the error.
pub fn expect_compile_err(source: &str, f: impl FnOnce(CompileError)) {
    match compile_source(source, None) {
        Err(err) => f(err),
        Ok(_) => panic!("expected compile error"),
    }
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

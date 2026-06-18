//! V0-065 `heap_dealloc` fixture build, verify, and VM semantics.

#![allow(clippy::expect_used)]

use phx_test::{ensure_built_project, require_cli_project};
use phx_vm::{VmErrorKind, run};

#[test]
fn heap_dealloc_fixture_runs() {
    require_cli_project("heap_dealloc");
    let built = ensure_built_project("heap_dealloc");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_dealloc");

    run(verified).expect("run heap_dealloc");
}

#[test]
fn heap_dealloc_unsafe_check_fails() {
    let root = require_cli_project("heap_dealloc_unsafe");
    let entry = root.join("src/main.phx");
    let err = phx_compiler::check_file(&entry).expect_err("dealloc outside unsafe");
    let msg = format!("{err}");
    assert!(
        msg.contains("unsafe") || msg.contains("IntrinsicRequiresUnsafe"),
        "expected unsafe diagnostic, got:\n{msg}"
    );
}

#[test]
fn heap_dealloc_double_free_fails_at_runtime() {
    require_cli_project("heap_dealloc_double");
    let built = ensure_built_project("heap_dealloc_double");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_dealloc_double");
    let err = run(verified).expect_err("double free should fail");
    assert!(
        matches!(err.kind, VmErrorKind::DoubleFree),
        "expected DoubleFree, got {err:?}"
    );
}

#[test]
fn heap_uaf_read_after_free_fails_at_runtime() {
    require_cli_project("heap_uaf");
    let built = ensure_built_project("heap_uaf");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_uaf");
    let err = run(verified).expect_err("use after free should fail");
    assert!(
        matches!(err.kind, VmErrorKind::UseAfterFree),
        "expected UseAfterFree, got {err:?}"
    );
}

#[test]
fn heap_drop_dealloc_fixture_runs() {
    require_cli_project("heap_drop_dealloc");
    let built = ensure_built_project("heap_drop_dealloc");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_drop_dealloc");

    run(verified).expect("run heap_drop_dealloc");
}

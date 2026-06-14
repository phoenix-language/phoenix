//! V0-065 `heap_dealloc` fixture build, verify, and VM semantics.

#![allow(clippy::expect_used)]

use phx_test::{cli_project, fixture_fs_lock, force_build_project};
use phx_vm::{VmErrorKind, run};

#[test]
fn heap_dealloc_fixture_runs() {
    let _lock = fixture_fs_lock();
    let root = cli_project("heap_dealloc");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("heap_dealloc");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_dealloc");

    run(verified).expect("run heap_dealloc");
}

#[test]
fn heap_dealloc_unsafe_check_fails() {
    let _lock = fixture_fs_lock();
    let root = cli_project("heap_dealloc_unsafe");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
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
    let _lock = fixture_fs_lock();
    let root = cli_project("heap_dealloc_double");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("heap_dealloc_double");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_dealloc_double");
    let err = run(verified).expect_err("double free should fail");
    assert!(
        matches!(err.kind, VmErrorKind::DoubleFree),
        "expected DoubleFree, got {err:?}"
    );
}

#[test]
fn heap_uaf_read_after_free_fails_at_runtime() {
    let _lock = fixture_fs_lock();
    let root = cli_project("heap_uaf");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("heap_uaf");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_uaf");
    let err = run(verified).expect_err("use after free should fail");
    assert!(
        matches!(err.kind, VmErrorKind::UseAfterFree),
        "expected UseAfterFree, got {err:?}"
    );
}

#[test]
fn heap_drop_dealloc_fixture_runs() {
    let _lock = fixture_fs_lock();
    let root = cli_project("heap_drop_dealloc");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("heap_drop_dealloc");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_drop_dealloc");

    run(verified).expect("run heap_drop_dealloc");
}

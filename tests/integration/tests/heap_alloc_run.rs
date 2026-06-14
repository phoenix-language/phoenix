//! V0-030 `heap_alloc` fixture build, verify, and VM semantics.

#![allow(clippy::expect_used)]

use phx_test::{
    ExpectedLocal, assert_main_locals, cli_project, fixture_fs_lock, force_build_project,
};
use phx_vm::run;

#[test]
fn heap_alloc_fixture_runs() {
    let _lock = fixture_fs_lock();
    let root = cli_project("heap_alloc");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("heap_alloc");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_alloc");

    run(verified).expect("run heap_alloc");
}

#[test]
fn heap_alloc_reads_seventy_seven_from_heap() {
    let _lock = fixture_fs_lock();
    let root = cli_project("heap_alloc");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("heap_alloc");
    // `v` local slot after `const v: u8 = *buf;`
    assert_main_locals(&built.module, &[(2, ExpectedLocal::U8(77))]);
}

#[test]
fn heap_alloc_unsafe_check_fails() {
    let _lock = fixture_fs_lock();
    let root = cli_project("heap_alloc_unsafe");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let entry = root.join("src/main.phx");
    let err = phx_compiler::check_file(&entry).expect_err("alloc outside unsafe");
    let msg = format!("{err}");
    assert!(
        msg.contains("unsafe") || msg.contains("IntrinsicRequiresUnsafe"),
        "expected unsafe diagnostic, got:\n{msg}"
    );
}

#[test]
fn heap_alloc_oom_with_low_heap_cap() {
    use phx_vm::{VmErrorKind, run_captured_with_heap_cap};

    let _lock = fixture_fs_lock();
    let root = cli_project("heap_alloc_oom");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("heap_alloc_oom");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_alloc_oom");
    let err = run_captured_with_heap_cap(verified, 32).expect_err("heap cap exceeded");
    assert_eq!(err.kind, VmErrorKind::OutOfMemory);
}

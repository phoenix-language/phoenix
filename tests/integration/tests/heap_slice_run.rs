//! V0-062 `heap_slice` fixture build, verify, and VM semantics.

#![allow(clippy::expect_used)]

use phx_test::{
    ExpectedLocal, assert_main_locals, cli_project, fixture_fs_lock, force_build_project,
};
use phx_vm::run;

#[test]
fn heap_slice_fixture_runs() {
    let _lock = fixture_fs_lock();
    let root = cli_project("heap_slice");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("heap_slice");
    phx_bytecode::verify(&built.module).expect("verify heap_slice");
    run(&built.module).expect("run heap_slice");
}

#[test]
fn heap_slice_reads_seventy_seven_via_index() {
    let _lock = fixture_fs_lock();
    let root = cli_project("heap_slice");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("heap_slice");
    // `v` local slot after `const v: u8 = sl[0];`
    assert_main_locals(&built.module, &[(3, ExpectedLocal::U8(77))]);
}

#[test]
fn heap_slice_unsafe_check_fails() {
    let _lock = fixture_fs_lock();
    let root = cli_project("heap_slice_unsafe");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let entry = root.join("src/main.phx");
    let err = phx_compiler::check_file(&entry).expect_err("slice outside unsafe");
    let msg = format!("{err}");
    assert!(
        msg.contains("unsafe") || msg.contains("IntrinsicRequiresUnsafe"),
        "expected unsafe diagnostic, got:\n{msg}"
    );
}

#[test]
fn heap_slice_oob_index_fails_at_runtime() {
    let _lock = fixture_fs_lock();
    let root = cli_project("heap_slice_oob");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("heap_slice_oob");
    phx_bytecode::verify(&built.module).expect("verify heap_slice_oob");
    let err = run(&built.module).expect_err("OOB slice index");
    let msg = format!("{err}");
    assert!(
        msg.contains("FieldOutOfRange") || msg.contains("out of range"),
        "expected OOB diagnostic, got:\n{msg}"
    );
}

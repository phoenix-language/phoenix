//! V0-062 `heap_slice` fixture build, verify, and VM semantics.

#![allow(clippy::expect_used)]

use phx_test::{
    ExpectedLocal, assert_main_locals, fixture_fs_lock, force_build_project, require_cli_project,
};
use phx_vm::run;

#[test]
fn heap_slice_fixture_runs() {
    let _lock = fixture_fs_lock();
    require_cli_project("heap_slice");
    let built = force_build_project("heap_slice");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_slice");

    run(verified).expect("run heap_slice");
}

#[test]
fn heap_slice_reads_seventy_seven_via_index() {
    let _lock = fixture_fs_lock();
    require_cli_project("heap_slice");
    let built = force_build_project("heap_slice");
    // `v` local slot after `const v: u8 = sl[0];`
    assert_main_locals(&built.module, &[(3, ExpectedLocal::U8(77))]);
}

#[test]
fn heap_slice_unsafe_check_fails() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("heap_slice_unsafe");
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
    require_cli_project("heap_slice_oob");
    let built = force_build_project("heap_slice_oob");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_slice_oob");
    let err = run(verified).expect_err("OOB slice index");
    let msg = format!("{err}");
    assert!(
        msg.contains("FieldOutOfRange") || msg.contains("out of range"),
        "expected OOB diagnostic, got:\n{msg}"
    );
}

#[test]
fn heap_slice_store_round_trip() {
    let _lock = fixture_fs_lock();
    require_cli_project("heap_slice_store");
    let built = force_build_project("heap_slice_store");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_slice_store");

    run(verified).expect("run heap_slice_store");
    // `v` local slot after `const v: u8 = sl[0];`
    assert_main_locals(&built.module, &[(3, ExpectedLocal::U8(42))]);
}

#[test]
fn heap_slice_nested_index_store_round_trip() {
    let _lock = fixture_fs_lock();
    require_cli_project("heap_slice_nested_index");
    let built = force_build_project("heap_slice_nested_index");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_slice_nested_index");

    run(verified).expect("run heap_slice_nested_index");
    assert_main_locals(&built.module, &[(3, ExpectedLocal::U8(42))]);
}

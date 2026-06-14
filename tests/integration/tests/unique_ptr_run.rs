//! `UniquePtr` std smoke fixtures.

#![allow(clippy::expect_used)]

use phx_test::{cli_project, fixture_fs_lock, force_build_project};
use phx_vm::run;

#[test]
fn unique_ptr_smoke_fixture_runs() {
    let _lock = fixture_fs_lock();
    let root = cli_project("unique_ptr_smoke");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("unique_ptr_smoke");
    let verified = phx_bytecode::verify(&built.module).expect("verify unique_ptr_smoke");

    run(verified).expect("run unique_ptr_smoke");
}

#[test]
fn unique_ptr_drop_smoke_fixture_runs() {
    let _lock = fixture_fs_lock();
    let root = cli_project("unique_ptr_drop_smoke");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("unique_ptr_drop_smoke");
    let verified = phx_bytecode::verify(&built.module).expect("verify unique_ptr_drop_smoke");

    run(verified).expect("run unique_ptr_drop_smoke");
}

//! `DynamicArray` std smoke fixtures.

#![allow(clippy::expect_used)]

use phx_test::{cli_project, fixture_fs_lock, force_build_project};
use phx_vm::run;

#[test]
fn dynamic_array_smoke_fixture_runs() {
    let _lock = fixture_fs_lock();
    let root = cli_project("dynamic_array_smoke");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("dynamic_array_smoke");
    phx_bytecode::verify(&built.module).expect("verify dynamic_array_smoke");
    run(&built.module).expect("run dynamic_array_smoke");
}

#[test]
fn dynamic_array_drop_smoke_fixture_runs() {
    let _lock = fixture_fs_lock();
    let root = cli_project("dynamic_array_drop_smoke");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("dynamic_array_drop_smoke");
    phx_bytecode::verify(&built.module).expect("verify dynamic_array_drop_smoke");
    run(&built.module).expect("run dynamic_array_drop_smoke");
}

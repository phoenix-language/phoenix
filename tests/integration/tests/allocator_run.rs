//! V0-066 `allocator_smoke` fixture — std `Allocator` trait over VM heap.

#![allow(clippy::expect_used)]

use phx_test::{cli_project, fixture_fs_lock, force_build_project};
use phx_vm::run;

#[test]
fn allocator_smoke_fixture_runs() {
    let _lock = fixture_fs_lock();
    let root = cli_project("allocator_smoke");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let built = force_build_project("allocator_smoke");
    phx_bytecode::verify(&built.module).expect("verify allocator_smoke");
    run(&built.module).expect("run allocator_smoke");
}

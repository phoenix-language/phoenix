//! V0-066 `allocator_smoke` fixture — std `Allocator` trait over VM heap.

#![allow(clippy::expect_used)]

use phx_test::{fixture_fs_lock, force_build_project, require_cli_project};
use phx_vm::run;

#[test]
fn allocator_smoke_fixture_runs() {
    let _lock = fixture_fs_lock();
    require_cli_project("allocator_smoke");
    let built = force_build_project("allocator_smoke");
    let verified = phx_bytecode::verify(&built.module).expect("verify allocator_smoke");

    run(verified).expect("run allocator_smoke");
}

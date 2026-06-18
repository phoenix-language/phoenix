//! V0-066 `allocator_smoke` fixture — std `Allocator` trait over VM heap.

#![allow(clippy::expect_used)]

use phx_test::{ensure_built_project, require_cli_project};
use phx_vm::run;

#[test]
fn allocator_smoke_fixture_runs() {
    require_cli_project("allocator_smoke");
    let built = ensure_built_project("allocator_smoke");
    let verified = phx_bytecode::verify(&built.module).expect("verify allocator_smoke");

    run(verified).expect("run allocator_smoke");
}

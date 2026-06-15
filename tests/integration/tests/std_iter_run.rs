//! V0-055 `std_iter` fixture build and VM run.

#![allow(clippy::expect_used)]

use phx_bytecode::verify;
use phx_test::{fixture_fs_lock, force_build_project};
use phx_vm::run;

#[test]
fn std_iter_fixture_runs() {
    let _lock = fixture_fs_lock();
    let built = force_build_project("std_iter");
    let verified = verify(&built.module).expect("verify std_iter");
    run(verified).expect("run std_iter");
}

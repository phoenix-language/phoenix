//! V0-060 `std_errors` fixture build and VM run.

#![allow(clippy::expect_used)]

use phx_bytecode::verify;
use phx_test::{ensure_built_project, require_cli_project};
use phx_vm::run;

#[test]
fn std_errors_fixture_runs() {
    require_cli_project("std_errors");
    let built = ensure_built_project("std_errors");
    let verified = verify(&built.module).expect("verify");

    run(verified).expect("run std_errors");
}

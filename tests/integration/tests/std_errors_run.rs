//! V0-060 `std_errors` fixture build and VM run.

#![allow(clippy::expect_used)]

use phx_bytecode::verify;
use phx_compiler::{BuildOptions, build_project, load_project_binary};
use phx_test::{cli_project, discover_cli_project, fixture_fs_lock};
use phx_vm::run;

#[test]
fn std_errors_fixture_runs() {
    let _lock = fixture_fs_lock();
    let root = cli_project("std_errors");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = discover_cli_project(&root);
    build_project(&config, None, BuildOptions::force(true)).expect("build std_errors");
    let module = load_project_binary(&config).expect("load bytecode");
    let verified = verify(&module).expect("verify");

    run(verified).expect("run std_errors");
}

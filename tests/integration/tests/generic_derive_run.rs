//! Generic `#[derive]` fixture build and VM run.

#![allow(clippy::expect_used)]

use phx_bytecode::verify;
use phx_compiler::{BuildOptions, build_project, load_project_binary};
use phx_test::{discover_cli_project, fixture_fs_lock, require_cli_project};
use phx_vm::run;

#[test]
fn std_generic_derive_fixture_runs() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("std_generic_derive");
    let config = discover_cli_project(&root);
    build_project(&config, None, BuildOptions::force(true)).expect("build std_generic_derive");
    let module = load_project_binary(&config).expect("load bytecode");
    let verified = verify(&module).expect("verify");

    run(verified).expect("run std_generic_derive");
}

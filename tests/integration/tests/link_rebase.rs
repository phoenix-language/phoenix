//! Link rebasing: module-local type/const pools merge with patched operands.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_bytecode::verify;
use phx_compiler::BuildOptions;
use phx_test::{build_cli_project, cli_project, discover_cli_project, fixture_fs_lock};
use phx_vm::run;

#[test]
fn link_rebase_cross_module_struct_field_and_enum_match() {
    let _lock = fixture_fs_lock();
    let root = cli_project("link_rebase");
    let config = discover_cli_project(&root);
    build_cli_project(&config, BuildOptions::force(true));

    let bin_path = root.join("build/bin/link_rebase.phx0");
    let bytes = std::fs::read(&bin_path).expect("read linked binary");
    let module = phx_bytecode::BytecodeModule::decode(&bytes).expect("decode");
    verify(&module).expect("verify linked binary");
    run(&module).expect("run cross-module struct field + enum match");
}

//! Block-scoped `#import` (V0-014) acceptance.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_compiler::CompileError;
use phx_test::{cli_modules_dir, compile_fixture_module};
use phx_vm::run;

#[test]
fn block_import_main_compiles_and_runs() {
    let root = cli_modules_dir();
    let module = compile_fixture_module("block_import_main.phx", &root);
    run(&module).expect("run block_import_main");
}

#[test]
fn block_import_nested_glob_compiles_and_runs() {
    let root = cli_modules_dir();
    let module = compile_fixture_module("block_import_nested.phx", &root);
    run(&module).expect("run block_import_nested");
}

#[test]
fn block_import_unresolved_outside_block() {
    let root = cli_modules_dir();
    let path = root.join("block_import_outside.phx");
    let err = phx_compiler::compile_to_module_with_module_path(&path, &root)
        .expect_err("add should be unresolved outside import block");
    let CompileError::Resolve { bag, .. } = err else {
        panic!("expected resolve error, got {err:?}");
    };
    let msg = bag.to_string();
    assert!(
        msg.contains("unresolved identifier") || msg.contains("add"),
        "expected unresolved name diagnostic: {msg}"
    );
}

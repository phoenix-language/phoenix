//! `std::text::fmt::print` display-buffer stdout bridge.

#![allow(clippy::expect_used)]

use std::process::Stdio;

use phx_bytecode::verify;
use phx_test::{fixture_fs_lock, force_build_project, phx_bin_path, require_cli_project};
use phx_vm::{register_builtin_foreign_stubs, run};

#[test]
fn print_s32_fixture_runs() {
    let _lock = fixture_fs_lock();
    register_builtin_foreign_stubs();
    require_cli_project("print_s32");
    let built = force_build_project("print_s32");
    let verified = verify(&built.module).expect("verify print_s32");

    run(verified).expect("run print_s32");
}

#[test]
fn print_s32_cli_writes_stdout() {
    let _lock = fixture_fs_lock();
    let root = require_cli_project("print_s32");
    force_build_project("print_s32");
    let output = std::process::Command::new(phx_bin_path())
        .args(["run", "--no-build", "--project-root"])
        .arg(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn phx run");
    assert!(
        output.status.success(),
        "phx run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"42");
}

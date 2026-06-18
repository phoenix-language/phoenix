//! Pre-scheduler stdout bridge (`std::io::write_stdout`).

#![allow(clippy::expect_used)]

use std::process::Stdio;

use phx_bytecode::verify;
use phx_test::{
    ensure_built_project, ensure_built_project_unlocked, phx_bin_path, require_cli_project,
    with_project_fs_lock,
};
use phx_vm::{register_builtin_foreign_stubs, run};

#[test]
fn hello_print_fixture_runs() {
    register_builtin_foreign_stubs();
    require_cli_project("hello_print");
    let built = ensure_built_project("hello_print");
    let verified = verify(&built.module).expect("verify hello_print");

    run(verified).expect("run hello_print");
}

#[test]
fn hello_print_cli_writes_stdout() {
    with_project_fs_lock("hello_print", || {
        let root = require_cli_project("hello_print");
        ensure_built_project_unlocked("hello_print");
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
        assert_eq!(output.stdout, b"hello\n");
    });
}

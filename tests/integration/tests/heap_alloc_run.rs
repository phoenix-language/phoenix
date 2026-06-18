//! V0-030 `heap_alloc` fixture build, verify, and VM semantics.

#![allow(clippy::expect_used)]

use phx_test::{
    ExpectedLocal, PhxCli, assert_main_locals, ensure_built_project, ensure_built_project_unlocked,
    require_cli_project, with_project_fs_lock,
};
use phx_vm::run;

#[test]
fn heap_alloc_fixture_runs() {
    require_cli_project("heap_alloc");
    let built = ensure_built_project("heap_alloc");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_alloc");

    run(verified).expect("run heap_alloc");
}

#[test]
fn heap_alloc_reads_seventy_seven_from_heap() {
    require_cli_project("heap_alloc");
    let built = ensure_built_project("heap_alloc");
    // `v` local slot after `const v: u8 = *buf;`
    assert_main_locals(&built.module, &[(2, ExpectedLocal::U8(77))]);
}

#[test]
fn heap_alloc_unsafe_check_fails() {
    let root = require_cli_project("heap_alloc_unsafe");
    let entry = root.join("src/main.phx");
    let err = phx_compiler::check_file(&entry).expect_err("alloc outside unsafe");
    let msg = format!("{err}");
    assert!(
        msg.contains("unsafe") || msg.contains("IntrinsicRequiresUnsafe"),
        "expected unsafe diagnostic, got:\n{msg}"
    );
}

#[test]
fn heap_alloc_oom_with_low_heap_cap() {
    use phx_vm::{VmErrorKind, run_captured_with_heap_cap};

    require_cli_project("heap_alloc_oom");
    let built = ensure_built_project("heap_alloc_oom");
    let verified = phx_bytecode::verify(&built.module).expect("verify heap_alloc_oom");
    let err = run_captured_with_heap_cap(verified, 32).expect_err("heap cap exceeded");
    assert_eq!(err.kind, VmErrorKind::OutOfMemory);
}

#[test]
fn heap_alloc_oom_cli_heap_cap_flag() {
    with_project_fs_lock("heap_alloc_oom", || {
        let root = require_cli_project("heap_alloc_oom");
        ensure_built_project_unlocked("heap_alloc_oom");
        let cli = PhxCli::ensure_built();
        let root_arg = root.to_string_lossy();
        cli.run(&[
            "run",
            "--no-build",
            "--project-root",
            &root_arg,
            "--heap-cap",
            "32",
        ])
        .assert_failure()
        .assert_contains("heap allocation exceeded cap");
    });
}

#[test]
fn heap_alloc_oom_cli_heap_cap_equals_form() {
    with_project_fs_lock("heap_alloc_oom", || {
        let root = require_cli_project("heap_alloc_oom");
        ensure_built_project_unlocked("heap_alloc_oom");
        let cli = PhxCli::ensure_built();
        let root_arg = root.to_string_lossy();
        cli.run(&[
            "run",
            "--no-build",
            "--project-root",
            &root_arg,
            "--heap-cap=32",
        ])
        .assert_failure()
        .assert_contains("heap allocation exceeded cap");
    });
}

#[test]
fn heap_alloc_oom_from_phoenix_toml_vm_section() {
    with_project_fs_lock("heap_alloc_oom", || {
        let root = require_cli_project("heap_alloc_oom");
        ensure_built_project_unlocked("heap_alloc_oom");
        let cli = PhxCli::ensure_built();
        let root_arg = root.to_string_lossy();
        cli.run(&["run", "--no-build", "--project-root", &root_arg])
            .assert_failure()
            .assert_contains("heap allocation exceeded cap");
    });
}

#[test]
fn heap_alloc_cli_heap_cap_suffix_runs_ok() {
    with_project_fs_lock("heap_alloc", || {
        let root = require_cli_project("heap_alloc");
        ensure_built_project_unlocked("heap_alloc");
        let cli = PhxCli::ensure_built();
        let root_arg = root.to_string_lossy();
        cli.run(&[
            "run",
            "--no-build",
            "--project-root",
            &root_arg,
            "--heap-cap",
            "64mb",
        ])
        .assert_success();
    });
}

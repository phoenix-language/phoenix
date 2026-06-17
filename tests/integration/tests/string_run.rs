//! `String` std semantics fixtures.

#![allow(clippy::expect_used)]

use phx_test::{fixture_fs_lock, force_build_project, require_cli_project};
use phx_vm::run;

#[test]
fn string_smoke_fixture_runs() {
    let _lock = fixture_fs_lock();
    require_cli_project("string_smoke");
    let built = force_build_project("string_smoke");
    let verified = phx_bytecode::verify(&built.module).expect("verify string_smoke");

    run(verified).expect("run string_smoke");
}

#[test]
fn string_clone_fixture_runs() {
    let _lock = fixture_fs_lock();
    require_cli_project("string_clone");
    let built = force_build_project("string_clone");
    let verified = phx_bytecode::verify(&built.module).expect("verify string_clone");

    run(verified).expect("run string_clone");
}

#[test]
fn string_partialeq_fixture_runs() {
    let _lock = fixture_fs_lock();
    require_cli_project("string_partialeq");
    let built = force_build_project("string_partialeq");
    let verified = phx_bytecode::verify(&built.module).expect("verify string_partialeq");

    run(verified).expect("run string_partialeq");
}

#[test]
fn string_fmt_fixture_runs() {
    let _lock = fixture_fs_lock();
    require_cli_project("string_fmt");
    let built = force_build_project("string_fmt");
    let verified = phx_bytecode::verify(&built.module).expect("verify string_fmt");

    run(verified).expect("run string_fmt");
}

#[test]
fn string_fmt_bool_fixture_runs() {
    let _lock = fixture_fs_lock();
    require_cli_project("string_fmt_bool");
    let built = force_build_project("string_fmt_bool");
    let verified = phx_bytecode::verify(&built.module).expect("verify string_fmt_bool");

    run(verified).expect("run string_fmt_bool");
}

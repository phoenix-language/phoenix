//! V0-063 trait default body inheritance — build, verify, and VM semantics.

#![allow(clippy::expect_used)]

use phx_test::{ExpectedLocal, assert_main_locals, ensure_built_project, require_cli_project};
use phx_vm::run;

#[test]
fn trait_default_fixture_runs() {
    require_cli_project("trait_default");
    let built = ensure_built_project("trait_default");
    let verified = phx_bytecode::verify(&built.module).expect("verify trait_default");

    run(verified).expect("run trait_default");
}

#[test]
fn trait_default_inherited_zero() {
    require_cli_project("trait_default");
    let built = ensure_built_project("trait_default");
    // `n` local after inherited `Counter::zero()` default
    assert_main_locals(&built.module, &[(2, ExpectedLocal::S32(0))]);
}

#[test]
fn trait_default_override_fixture_runs() {
    require_cli_project("trait_default_override");
    let built = ensure_built_project("trait_default_override");
    let verified = phx_bytecode::verify(&built.module).expect("verify trait_default_override");

    run(verified).expect("run trait_default_override");
}

#[test]
fn trait_into_from_default_fixture_runs() {
    require_cli_project("trait_into_from_default");
    let built = ensure_built_project("trait_into_from_default");
    let verified = phx_bytecode::verify(&built.module).expect("verify trait_into_from_default");

    run(verified).expect("run trait_into_from_default");
}

#[test]
fn trait_into_from_default_reads_forty_two() {
    require_cli_project("trait_into_from_default");
    let built = ensure_built_project("trait_into_from_default");
    assert_main_locals(&built.module, &[(2, ExpectedLocal::S32(42))]);
}

#[test]
fn trait_default_override_uses_explicit_impl() {
    require_cli_project("trait_default_override");
    let built = ensure_built_project("trait_default_override");
    assert_main_locals(&built.module, &[(2, ExpectedLocal::S32(99))]);
}

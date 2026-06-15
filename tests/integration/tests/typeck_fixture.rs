//! Type-check tests using on-disk fixtures (migrated from phx-compiler/tests/typeck.rs).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_compiler::{CompileError, check_file, compile_source};
use phx_diagnostics::TypeCheckError;
use phx_test::{cli_fixture, cli_project_main, require_fixture_file};

#[test]
fn question_mark_ok_with_std_imports() {
    let path = cli_project_main("std_try");
    let unit = check_file(&path).unwrap_or_else(|e| panic!("std_try typeck: {e}"));
    assert!(
        !unit.typed.try_sites.is_empty(),
        "expected try_sites in std_try fixture"
    );
}

#[test]
fn try_result_from_conversion_ok() {
    let path = cli_project_main("std_try_from");
    check_file(&path).unwrap_or_else(|e| panic!("std_try_from typeck: {e}"));
}

#[test]
fn try_result_from_missing() {
    let path = cli_project_main("std_try_from_missing");
    let err = check_file(&path).expect_err("expected type error");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| { matches!(&e.error, TypeCheckError::TryErrorFromMissing { .. }) }),
        "expected TryErrorFromMissing: {:?}",
        bag.errors()
    );
}

#[test]
fn try_result_ok_mismatch() {
    let path = cli_project_main("std_try_ok_mismatch");
    let err = check_file(&path).expect_err("expected type error");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| { matches!(&e.error, TypeCheckError::InvalidTryOperand { .. }) }),
        "expected InvalidTryOperand for Ok type mismatch: {:?}",
        bag.errors()
    );
}

#[test]
fn try_result_identical_err_regression() {
    let path = cli_project_main("std_try");
    check_file(&path).unwrap_or_else(|e| panic!("std_try typeck: {e}"));
}

#[test]
fn generic_cli_fixtures_check_file_ok() {
    for name in ["generic_fn.phx", "generic_struct.phx", "generic_enum.phx"] {
        let fixture = cli_fixture(name);
        let path = require_fixture_file(&fixture);
        let source = std::fs::read_to_string(path).expect("read fixture");
        compile_source(&source, Some(path))
            .unwrap_or_else(|e| panic!("compile_source {name}: {e}"));
        check_file(path).unwrap_or_else(|e| panic!("check_file {name}: {e}"));
    }
}

#[test]
fn std_traits_fixture_typechecks() {
    let path = cli_project_main("std_traits");
    let unit = check_file(&path).unwrap_or_else(|e| panic!("std_traits typeck: {e}"));
    assert!(
        !unit.typed.primitive_method_sites.is_empty(),
        "expected primitive eq/clone method sites in std_traits"
    );
}

#[test]
fn std_prelude_fixture_typechecks_without_imports() {
    let path = cli_project_main("std_prelude");
    check_file(&path).unwrap_or_else(|e| panic!("std_prelude typeck: {e}"));
}

#[test]
fn dealloc_bytes_requires_unsafe() {
    let path = cli_project_main("heap_dealloc_unsafe");
    let err = check_file(&path).expect_err("expected dealloc_bytes outside unsafe to fail");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::IntrinsicRequiresUnsafe { .. })),
        "expected IntrinsicRequiresUnsafe: {:?}",
        bag.errors()
    );
}

#[test]
fn heap_dealloc_fixture_typechecks() {
    let path = cli_project_main("heap_dealloc");
    let unit = check_file(&path).expect("heap_dealloc typecheck");
    assert!(
        unit.typed.intrinsic_call_sites.len() >= 2,
        "expected at least alloc_bytes + dealloc_bytes intrinsic sites, got {}",
        unit.typed.intrinsic_call_sites.len()
    );
}

#[test]
fn alloc_bytes_requires_unsafe() {
    let path = cli_project_main("heap_alloc_unsafe");
    let err = check_file(&path).expect_err("expected alloc_bytes outside unsafe to fail");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::IntrinsicRequiresUnsafe { .. })),
        "expected IntrinsicRequiresUnsafe: {:?}",
        bag.errors()
    );
}

#[test]
fn slice_from_raw_parts_requires_unsafe() {
    let path = cli_project_main("heap_slice_unsafe");
    let err = check_file(&path).expect_err("expected slice_from_raw_parts outside unsafe to fail");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::IntrinsicRequiresUnsafe { .. })),
        "expected IntrinsicRequiresUnsafe: {:?}",
        bag.errors()
    );
}

#[test]
fn heap_slice_fixture_typechecks() {
    let path = cli_project_main("heap_slice");
    let unit = check_file(&path).expect("heap_slice typecheck");
    assert!(
        unit.typed.intrinsic_call_sites.len() >= 2,
        "expected at least alloc_bytes + slice_from_raw_parts intrinsic sites, got {}",
        unit.typed.intrinsic_call_sites.len()
    );
}

#[test]
fn heap_alloc_fixture_typechecks() {
    let path = cli_project_main("heap_alloc");
    let unit = check_file(&path).expect("heap_alloc typecheck");
    assert!(
        !unit.typed.intrinsic_call_sites.is_empty(),
        "expected intrinsic_call_sites for alloc_bytes"
    );
    // Reuses `buf` after `*buf = …` — would fail use-after-move if *mut u8 were non-Copyable.
}

#[test]
fn allocator_smoke_unsafe_fail_fixture() {
    let path = cli_project_main("allocator_smoke_unsafe_fail");
    let err = check_file(&path).expect_err("expected alloc outside unsafe to fail");
    let bag = match err {
        CompileError::TypeCheck { bag, .. } => bag,
        other => panic!("expected type-check error, got {other}"),
    };
    assert!(
        bag.errors()
            .iter()
            .any(|e| matches!(&e.error, TypeCheckError::UnsafeFnCallRequiresUnsafe { .. })),
        "expected UnsafeFnCallRequiresUnsafe: {:?}",
        bag.errors()
    );
}

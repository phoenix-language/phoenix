//! PHX-070 phase 2: CLI maps VM faults to Phoenix source spans via section 5.

#![allow(clippy::expect_used)]

use std::path::Path;

use phx_bytecode::{PcSpanTable, verify};
use phx_cli::vm_diag::{SourceContext, format_vm_error};
use phx_compiler::{
    compile_source,
    unstable::{codegen, lower},
};
use phx_test::{
    ensure_built_project, force_build_project_unlocked, require_cli_project, shared_cli,
    with_project_fs_lock,
};
use phx_vm::{VmErrorKind, run};

#[test]
fn nested_helper_runtime_error_maps_to_helper_source_line() {
    require_cli_project("nested_trap");
    let built = ensure_built_project("nested_trap");
    let verified = verify(&built.module).expect("verify nested_trap");
    let err = run(verified).expect_err("nested use after free should fail");
    assert!(
        matches!(err.kind, VmErrorKind::UseAfterFree),
        "expected UseAfterFree, got {err:?}"
    );

    let project = require_cli_project("nested_trap");
    let entry = project.join("src/main.phx");
    let ctx = SourceContext {
        project_root: Some(&project),
        entry_path: Some(&entry),
        entry_source: None,
    };
    let msg = format_vm_error(&built.module, &err, &ctx);
    assert!(
        msg.contains("use after free")
            && msg.contains("read_after_free")
            && msg.contains("src/main.phx:7:"),
        "expected helper function name and source line in formatted error, got:\n{msg}"
    );
    assert!(
        !msg.contains("(function"),
        "should not fall back to bytecode site when debug section present, got:\n{msg}"
    );
}

#[test]
fn nested_helper_cli_shows_function_name_and_source_line_on_stderr() {
    let project = require_cli_project("nested_trap");
    let out = shared_cli().run_project_fails(&project);
    out.assert_contains("runtime error:");
    out.assert_contains("use after free");
    out.assert_contains("read_after_free");
    out.assert_contains("src/main.phx:7:");
}

#[test]
fn indirect_call_runtime_error_maps_to_callee_source_line() {
    let source = r"div_zero :: (a: s32, b: s32) => s32 {
    a / b
};

invoke :: (f: :: (s32, s32) => s32, x: s32, y: s32) => s32 {
    f(x, y)
};

main :: () => {
    const n: s32 = invoke(div_zero, 1, 0);
    const _ = n;
};
";
    let path = Path::new("tests/cli/fixtures/indirect_trap.phx");
    let unit = compile_source(source, Some(path)).expect("compile indirect_trap");
    let module = codegen(&lower(&unit.typed).expect("lower"), &unit.typed).expect("codegen");
    verify(&module).expect("verify indirect_trap");
    let err = run(verify(&module).expect("verify")).expect_err("division by zero");
    assert!(
        matches!(err.kind, VmErrorKind::DivisionByZero),
        "expected DivisionByZero, got {err:?}"
    );

    let ctx = SourceContext {
        project_root: Some(Path::new("tests/cli/fixtures")),
        entry_path: Some(path),
        entry_source: Some(source),
    };
    let msg = format_vm_error(&module, &err, &ctx);
    assert!(
        msg.contains("division by zero in div_zero at indirect_trap.phx:2:"),
        "expected callee div_zero line and name, got:\n{msg}"
    );
}

#[test]
fn heap_uaf_runtime_error_maps_to_source_span() {
    require_cli_project("heap_uaf");
    let built = ensure_built_project("heap_uaf");
    let verified = verify(&built.module).expect("verify heap_uaf");
    let err = run(verified).expect_err("use after free should fail");
    assert!(
        matches!(err.kind, VmErrorKind::UseAfterFree),
        "expected UseAfterFree, got {err:?}"
    );

    let project = require_cli_project("heap_uaf");
    let entry = project.join("src/main.phx");
    let ctx = SourceContext {
        project_root: Some(&project),
        entry_path: Some(&entry),
        entry_source: None,
    };
    let msg = format_vm_error(&built.module, &err, &ctx);
    assert!(
        msg.contains("use after free") && msg.contains("src/main.phx:"),
        "expected source span in formatted error, got:\n{msg}"
    );
    assert!(
        !msg.contains("(function"),
        "should not fall back to bytecode site when debug section present, got:\n{msg}"
    );
}

#[test]
fn heap_uaf_cli_shows_source_span_on_stderr() {
    with_project_fs_lock("heap_uaf", || {
        let project = require_cli_project("heap_uaf");
        force_build_project_unlocked("heap_uaf");
        let out = shared_cli().run_no_build_project_fails(&project);
        out.assert_contains("runtime error:");
        out.assert_contains("use after free");
        out.assert_contains("src/main.phx:");
    });
}

#[test]
fn vm_error_without_debug_section_falls_back_to_bytecode_site() {
    require_cli_project("heap_uaf");
    let built = ensure_built_project("heap_uaf");
    let mut module = built.module.clone();
    module.pc_spans = PcSpanTable::default();
    module.header.flags &= !phx_bytecode::PHX0_HAS_DEBUG;
    module.header.section_count = 5;

    let verified = verify(&module).expect("verify stripped module");
    let err = run(verified).expect_err("use after free should fail");
    let ctx = SourceContext {
        project_root: Some(&require_cli_project("heap_uaf")),
        entry_path: None,
        entry_source: None,
    };
    let msg = format_vm_error(&module, &err, &ctx);
    assert!(
        msg.contains("(function") && msg.contains("pc"),
        "expected bytecode site fallback, got:\n{msg}"
    );
}

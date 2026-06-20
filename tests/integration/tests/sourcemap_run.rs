//! PHX-070 phase 2: CLI maps VM faults to Phoenix source spans via section 5.

#![allow(clippy::expect_used)]

use phx_bytecode::verify;
use phx_cli::vm_diag::{SourceContext, format_vm_error};
use phx_test::{ensure_built_project, require_cli_project, shared_cli};
use phx_vm::{VmErrorKind, run};

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
    let project = require_cli_project("heap_uaf");
    let out = shared_cli().run_project_fails(&project);
    out.assert_contains("runtime error:");
    out.assert_contains("use after free");
    out.assert_contains("src/main.phx:");
}

#[test]
fn vm_error_without_debug_section_falls_back_to_bytecode_site() {
    require_cli_project("heap_uaf");
    let built = ensure_built_project("heap_uaf");
    let mut module = built.module.clone();
    module.pc_spans = Default::default();
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

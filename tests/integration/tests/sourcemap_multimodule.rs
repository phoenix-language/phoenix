//! PHX-070 phase 6: linked multi-module bytecode maps VM traps to callee source spans.

#![allow(clippy::expect_used)]

use phx_bytecode::verify;
use phx_cli::vm_diag::{SourceContext, format_vm_error};
use phx_test::{ensure_built_project, require_cli_project, shared_cli};
use phx_vm::{VmErrorKind, run};

#[test]
fn callee_module_runtime_error_maps_to_util_source_line() {
    require_cli_project("modules_trap");
    let built = ensure_built_project("modules_trap");
    let verified = verify(&built.module).expect("verify modules_trap");
    let err = run(verified).expect_err("division by zero should fail");
    assert!(
        matches!(err.kind, VmErrorKind::DivisionByZero),
        "expected DivisionByZero, got {err:?}"
    );

    let project = require_cli_project("modules_trap");
    let entry = project.join("src/main.phx");
    let ctx = SourceContext {
        project_root: Some(&project),
        entry_path: Some(&entry),
        entry_source: None,
    };
    let msg = format_vm_error(&built.module, &err, &ctx);
    assert!(
        msg.contains("division by zero in div_zero at") && msg.contains("util/trap.phx:2:"),
        "expected merged callee function debug name and util/trap line, got:\n{msg}"
    );
    assert!(
        !msg.contains("src/main.phx:"),
        "should cite callee module, not main entry, got:\n{msg}"
    );
    assert!(
        !msg.contains("(function"),
        "should not fall back to bytecode site when debug section present, got:\n{msg}"
    );
}

#[test]
fn callee_module_cli_shows_util_source_line_on_stderr() {
    let project = require_cli_project("modules_trap");
    let out = shared_cli().run_project_fails(&project);
    out.assert_contains("runtime error:");
    out.assert_contains("division by zero");
    out.assert_contains("div_zero");
    out.assert_contains("util/trap.phx:2:");
}

#[test]
fn linked_module_merged_function_debug_names_survive_link() {
    require_cli_project("modules_trap");
    let built = ensure_built_project("modules_trap");
    assert!(
        built
            .module
            .pc_spans
            .function_names
            .iter()
            .any(|entry| { entry.name == "div_zero" || entry.name == "main" }),
        "dev link should merge function debug names from all modules, got: {:?}",
        built
            .module
            .pc_spans
            .function_names
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>()
    );
}

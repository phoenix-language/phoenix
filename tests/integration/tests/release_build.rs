//! PHX-070-p4: `phx build --release` strips PHX0 section 5.

#![allow(clippy::expect_used)]

use phx_bytecode::{PHX0_HAS_DEBUG, verify};
use phx_cli::vm_diag::{SourceContext, format_vm_error};
use phx_compiler::{BuildOptions, BuildProfile};
use phx_test::{
    discover_cli_project, force_built_project_with_options, load_built_binary, require_cli_project,
    shared_cli,
};
use phx_vm::{VmErrorKind, run};

fn path_to_arg(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn release_build_strips_section_5_and_debug_flag() {
    let built = force_built_project_with_options(
        "heap_uaf",
        BuildOptions::force(true).with_profile(BuildProfile::Release),
    );
    assert!(
        built.module.pc_spans.entries.is_empty(),
        "release build should omit PC span rows"
    );
    assert_eq!(
        built.module.header.flags & PHX0_HAS_DEBUG,
        0,
        "release build should clear PHX0_HAS_DEBUG"
    );
    assert_eq!(
        built.module.header.section_count, 5,
        "release build should omit section 5"
    );
    verify(&built.module).expect("release module should verify");
}

#[test]
fn dev_build_keeps_section_5_for_contrast() {
    let built = force_built_project_with_options(
        "heap_uaf",
        BuildOptions::force(true).with_profile(BuildProfile::Dev),
    );
    assert!(
        !built.module.pc_spans.entries.is_empty(),
        "dev build should keep PC span rows"
    );
    assert_ne!(
        built.module.header.flags & PHX0_HAS_DEBUG,
        0,
        "dev build should set PHX0_HAS_DEBUG"
    );
    verify(&built.module).expect("dev module should verify");
}

#[test]
fn release_build_cli_strips_section_5() {
    let project = require_cli_project("heap_uaf");
    shared_cli()
        .run(&[
            "build",
            "--release",
            "--build",
            "--project-root",
            &path_to_arg(&project),
        ])
        .assert_success();

    let config = discover_cli_project(&project);
    let module = load_built_binary(&config).expect("load release heap_uaf binary");
    assert!(module.pc_spans.entries.is_empty());
    assert_eq!(module.header.flags & PHX0_HAS_DEBUG, 0);
    verify(&module).expect("CLI release module should verify");
}

#[test]
fn release_build_runtime_error_falls_back_to_bytecode_site() {
    let built = force_built_project_with_options(
        "heap_uaf",
        BuildOptions::force(true).with_profile(BuildProfile::Release),
    );
    let verified = verify(&built.module).expect("verify release heap_uaf");
    let err = run(verified).expect_err("use after free should fail");
    assert!(
        matches!(err.kind, VmErrorKind::UseAfterFree),
        "expected UseAfterFree, got {err:?}"
    );

    let ctx = SourceContext {
        project_root: Some(&require_cli_project("heap_uaf")),
        entry_path: None,
        entry_source: None,
    };
    let msg = format_vm_error(&built.module, &err, &ctx);
    assert!(
        msg.contains("(function") && msg.contains("pc"),
        "expected bytecode site fallback without section 5, got:\n{msg}"
    );
    assert!(
        !msg.contains("src/main.phx:"),
        "release build should not cite Phoenix source spans, got:\n{msg}"
    );
}

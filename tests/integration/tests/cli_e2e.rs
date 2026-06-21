//! CLI end-to-end tests (subprocess `phx` binary).
//!
//! Covers CLI-only behavior: flags, project discovery, path dependencies, and
//! user-facing output. Compile/run semantics for embedded fixtures live in
//! other integration binaries (`run_smoke`, `run_*`, `diagnostics`).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_test::{
    cli_fixture, cli_modules_dir, cli_project, project_bin_path, repo_root, shared_cli,
    std_fs_lock, with_project_fs_lock,
};

fn path_to_arg(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

const DEPRECATED_LINT_NEEDLE: &str = "use of deprecated item";

#[test]
fn check_deprecated_warn_emits_warning() {
    let cli = shared_cli();
    cli.run(&[
        "check",
        &path_to_arg(&cli_fixture("attr_deprecated_warn.phx")),
    ])
    .assert_success()
    .assert_contains(DEPRECATED_LINT_NEEDLE);
}

#[test]
fn check_deprecated_deny_fails() {
    let cli = shared_cli();
    cli.run(&[
        "check",
        "--deny",
        &path_to_arg(&cli_fixture("attr_deprecated_warn.phx")),
    ])
    .assert_failure()
    .assert_contains(DEPRECATED_LINT_NEEDLE)
    .assert_contains("denied");
}

#[test]
fn check_deprecated_allow_passes_with_deny() {
    let cli = shared_cli();
    cli.run(&[
        "check",
        "--deny",
        &path_to_arg(&cli_fixture("attr_deprecated_allow.phx")),
    ])
    .assert_success();
}

#[test]
fn check_project_lint_deny_from_phoenix_toml() {
    let cli = shared_cli();
    cli.check_fails(&cli_project("lint_deny_project").join("src/main.phx"))
        .assert_contains("denied");
}

#[test]
fn compile_deprecated_warn_emits_warning() {
    let cli = shared_cli();
    let path = cli_fixture("attr_deprecated_warn.phx");
    let out = std::env::temp_dir().join("phx_test_attr_deprecated_warn.phx0");
    let _ = std::fs::remove_file(&out);
    cli.run(&["compile", &path_to_arg(&path), "-o", &path_to_arg(&out)])
        .assert_success()
        .assert_contains(DEPRECATED_LINT_NEEDLE);
    assert!(out.is_file());
    let _ = std::fs::remove_file(out);
}

#[test]
fn run_deprecated_warn_emits_warning() {
    let cli = shared_cli();
    cli.run(&[
        "run",
        &path_to_arg(&cli_fixture("attr_deprecated_warn.phx")),
    ])
    .assert_success()
    .assert_contains(DEPRECATED_LINT_NEEDLE);
}

#[test]
fn check_bad_type_shows_caret() {
    let cli = shared_cli();
    cli.check_fails(&cli_fixture("bad_type.phx"))
        .assert_contains_all(&["^", "-->"]);
}

#[test]
fn check_modules_cycle_fails() {
    let cli = shared_cli();
    let modules = cli_modules_dir();
    let out = cli.check_module_fails(&modules, &modules.join("cycle_a.phx"));
    out.assert_contains("cycle")
        .assert_contains_all(&["cycle_a", "cycle_b"]);
    assert!(
        out.combined.contains("E1008") || out.combined.contains("circular module import"),
        "got:\n{}",
        out.combined
    );
}

#[test]
fn check_project_entry_via_discovery() {
    let cli = shared_cli();
    cli.check_ok(&cli_project("project").join("src/main.phx"));
}

#[test]
fn run_rejects_stray_project_file() {
    let cli = shared_cli();
    cli.run_fails(&cli_project("project").join("src/util/math.phx"))
        .assert_contains("phoenix.toml");
}

#[test]
fn check_app_dep_without_prior_build() {
    let cli = shared_cli();
    cli.check_ok(&cli_project("app_dep").join("src/main.phx"));
}

#[test]
fn check_emit_interface_only() {
    let cli = shared_cli();
    cli.check_interface_only_project_smoke("project");
}

#[test]
fn build_emit_interface_only() {
    let cli = shared_cli();
    cli.build_interface_only_project_smoke("project");
}

#[test]
fn build_release_emit_interface_only() {
    with_project_fs_lock("project", || {
        let cli = shared_cli();
        let project = cli_project("project");
        let _ = std::fs::remove_dir_all(project.join("build"));
        cli.build_release_interface_ok(&project);
        assert!(project.join("build/manifest.json").is_file());
        assert!(
            project
                .join("build/pxi/cli_project_test/util/math.pxi")
                .is_file()
        );
        assert!(!project_bin_path().is_file());
        let manifest =
            std::fs::read_to_string(project.join("build/manifest.json")).expect("read manifest");
        assert!(
            manifest.contains(r#""profile": "release""#),
            "expected release profile in manifest, got:\n{manifest}"
        );
    });
}

#[test]
fn run_modules_main() {
    let cli = shared_cli();
    let modules = cli_modules_dir();
    cli.run_module_ok(&modules, &modules.join("main.phx"));
}

#[test]
fn build_project_default_entry() {
    with_project_fs_lock("project", || {
        let cli = shared_cli();
        let project = cli_project("project");
        let _ = std::fs::remove_dir_all(project.join("build"));
        cli.build_ok(&project);
        assert!(project_bin_path().is_file());
        assert!(project.join("build/manifest.json").is_file());
        assert!(
            project
                .join("build/pxi/cli_project_test/util/math.pxi")
                .is_file()
        );
    });
}

#[test]
fn run_no_build_project() {
    with_project_fs_lock("project", || {
        let cli = shared_cli();
        let project = cli_project("project");
        let _ = std::fs::remove_dir_all(project.join("build"));
        cli.build_ok(&project);
        cli.run_no_build_ok(&project);
        cli.run_no_build_entry_ok(&project, &project.join("src/main.phx"));
    });
}

#[test]
fn build_math_lib() {
    with_project_fs_lock("math_lib", || {
        let cli = shared_cli();
        let project = cli_project("math_lib");
        let _ = std::fs::remove_dir_all(project.join("build"));
        cli.build_ok(&project);
        assert!(project.join("build/lib/math.phx0").is_file());
        assert!(project.join("build/pxi/math.pxi").is_file());
    });
}

#[test]
fn build_std() {
    let cli = shared_cli();
    let _lock = std_fs_lock();
    let project = repo_root().join("std");
    let build_dir = project.join("build");
    if build_dir.is_dir() {
        std::fs::remove_dir_all(&build_dir).expect("remove std/build");
    }
    cli.build_ok(&project);
    assert!(project.join("build/lib/std.phx0").is_file());
    assert!(project.join("build/pxi/std.pxi").is_file());
}

#[test]
fn build_app_dep() {
    with_project_fs_lock("app_dep", || {
        let cli = shared_cli();
        let project = cli_project("app_dep");
        let _ = std::fs::remove_dir_all(project.join("build"));
        cli.build_ok(&project);
        assert!(project.join("build/bin/app_dep.phx0").is_file());
        assert!(project.join("build/deps/math/lib/math.phx0").is_file());
        cli.run_no_build_ok(&project);
    });
}

#[test]
fn build_bad_dep_key_fails() {
    let cli = shared_cli();
    let out = cli.build_fails(&cli_project("bad_dep_key"));
    assert!(
        out.combined.contains("dependency key") || out.combined.contains("invalid phoenix.toml"),
        "got:\n{}",
        out.combined
    );
}

#[test]
fn build_bin_missing_main_fails() {
    let cli = shared_cli();
    let out = cli.build_fails(&cli_project("bin_missing_main"));
    assert!(
        out.combined.contains("main.phx") || out.combined.contains("invalid phoenix.toml"),
        "got:\n{}",
        out.combined
    );
}

#[test]
fn compile_writes_phx0() {
    let cli = shared_cli();
    let out = repo_root().join("target/phx-cli-sample.phx0");
    cli.compile_ok(&cli_fixture("sample.phx"), &out);
}

#[test]
fn help_shows_usage() {
    let cli = shared_cli();
    cli.help_ok();
}

#[test]
fn explain_known_code() {
    let cli = shared_cli();
    cli.run(&["explain", "E2001"])
        .assert_success()
        .assert_contains("type does not match");
}

#[test]
fn explain_unknown_code() {
    let cli = shared_cli();
    cli.run(&["explain", "E9999"])
        .assert_failure()
        .assert_contains("no explanation available");
}

#[test]
fn explain_phx_013_codes() {
    let cli = shared_cli();
    let cases = [
        ("E3002", "required token"),
        ("E3003", "not supported"),
        ("E3004", "pattern"),
        ("E3005", "intern"),
        ("E2033", "Drop"),
    ];
    for (code, needle) in cases {
        cli.run(&["explain", code])
            .assert_success()
            .assert_contains(needle);
    }
}

#[test]
fn explain_borrow_codes_e2047_e2048() {
    let cli = shared_cli();
    let cases = [
        ("E2047", "overlapping"),
        ("E2047", "&mut"),
        ("E2048", "shared"),
        ("E2048", "mutable"),
    ];
    for (code, needle) in cases {
        cli.run(&["explain", code])
            .assert_success()
            .assert_contains(needle);
    }
}

#[test]
fn panic_is_caught_without_rust_backtrace() {
    let cli = shared_cli();
    let out = cli.run_with_env(&["version"], &[("PHX_TEST_FORCE_PANIC", "1")]);
    out.assert_failure();
    assert!(
        !out.combined.contains("thread 'main' panicked"),
        "got:\n{}",
        out.combined
    );
    out.assert_contains("internal compiler error");
    assert!(
        !out.combined.contains("panic message:"),
        "default ICE mode must not leak panic detail; got:\n{}",
        out.combined
    );
    assert_eq!(out.status.code(), Some(6));
}

#[test]
fn panic_shows_debug_detail_when_phx_ice_debug_set() {
    let cli = shared_cli();
    let out = cli.run_with_env(
        &["version"],
        &[("PHX_TEST_FORCE_PANIC", "1"), ("PHX_ICE_DEBUG", "1")],
    );
    out.assert_failure();
    out.assert_contains("internal compiler error");
    out.assert_contains("panic message:");
    out.assert_contains("integration test forced panic");
    out.assert_contains("backtrace:");
    assert!(
        !out.combined.contains("thread 'main' panicked"),
        "Rust default panic hook must stay suppressed; got:\n{}",
        out.combined
    );
    assert_eq!(out.status.code(), Some(6));
}

fn rm_example_build(project_root: &std::path::Path) {
    let _ = std::fs::remove_dir_all(project_root.join("build"));
}

#[test]
fn examples_hello_print_run() {
    let cli = shared_cli();
    let root = cli.example_project("hello_print");
    rm_example_build(&root);
    cli.build_ok(&root);
    cli.run_no_build_ok(&root);
}

#[test]
fn examples_hello_dump_main() {
    let cli = shared_cli();
    let entry = "examples/hello/src/main.phx";
    cli.run(&["run", "--dump-main", entry])
        .assert_success()
        .assert_contains("main[")
        .assert_contains("U8(104)");
}

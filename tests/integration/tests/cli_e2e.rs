//! CLI end-to-end tests (subprocess `phx` binary).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_test::{
    NEG_CHECK_FIXTURES, PhxCli, SMOKE_FIXTURES, cli_fixture, cli_modules_dir,
    cli_project, project_bin_path, repo_root, rm_project_build, shared_cli,
};

fn cli() -> &'static PhxCli {
    shared_cli()
}

// --- check.sh ---

#[test]
fn check_sample_succeeds() {
    cli().check_fixture_ok("sample.phx");
}

#[test]
fn check_bad_type_shows_caret() {
    cli()
        .check_fails(&cli_fixture("bad_type.phx"))
        .assert_contains_all(&["^", "-->"]);
}

#[test]
fn check_missing_main_fails() {
    cli().check_fails(&cli_fixture("missing_main.phx"));
}

#[test]
fn check_negative_fixtures() {
    for (name, needle) in NEG_CHECK_FIXTURES {
        cli()
            .check_fails(&cli_fixture(name))
            .assert_contains(needle);
        if *name == "use_after_move.phx" {
            cli()
                .check_fails(&cli_fixture(name))
                .assert_contains("= note:");
        }
    }
}

#[test]
fn check_modules_main_succeeds() {
    let modules = cli_modules_dir();
    cli().check_module_ok(&modules, &modules.join("main.phx"));
}

#[test]
fn check_modules_list_and_glob_succeed() {
    let modules = cli_modules_dir();
    for entry in ["main_list.phx", "main_glob.phx"] {
        cli().check_module_ok(&modules, &modules.join(entry));
    }
}

#[test]
fn check_modules_duplicate_import_fails() {
    let modules = cli_modules_dir();
    let out = cli().check_module_fails(&modules, &modules.join("import_dup.phx"));
    assert!(
        out.combined.contains("duplicate")
            || out.combined.contains("Duplicate")
            || out.combined.contains("E1011"),
        "got:\n{}",
        out.combined
    );
}

#[test]
fn check_modules_private_import_fails() {
    let modules = cli_modules_dir();
    cli()
        .check_module_fails(&modules, &modules.join("import_private.phx"))
        .assert_contains("export");
}

#[test]
fn check_modules_cycle_fails() {
    let modules = cli_modules_dir();
    let out = cli().check_module_fails(&modules, &modules.join("cycle_a.phx"));
    out.assert_contains("cycle")
        .assert_contains_all(&["cycle_a", "cycle_b"]);
    assert!(
        out.combined.contains("E1008") || out.combined.contains("circular module import"),
        "got:\n{}",
        out.combined
    );
    assert!(
        out.combined.contains("#import") || out.combined.contains("import"),
        "got:\n{}",
        out.combined
    );
}

#[test]
fn check_project_entry_via_discovery() {
    cli().check_ok(&cli_project("project").join("src/main.phx"));
}

#[test]
fn run_rejects_stray_project_file() {
    cli()
        .run_fails(&cli_project("project").join("src/util/math.phx"))
        .assert_contains("phoenix.toml");
}

#[test]
fn check_lib_with_main_fails() {
    let out = cli().check_fails(&cli_project("lib_with_main").join("src/lib.phx"));
    assert!(
        out.combined.contains("not allowed in library") || out.combined.contains("E1014"),
        "got:\n{}",
        out.combined
    );
}

#[test]
fn check_app_dep_without_prior_build() {
    cli().check_ok(&cli_project("app_dep").join("src/main.phx"));
}

#[test]
fn check_emit_interface_only() {
    let _lock = phx_test::fixture_fs_lock();
    rm_project_build("project");
    let entry = cli_project("project").join("src/main.phx");
    cli().check_interface_ok(&entry);
    let project = cli_project("project");
    assert!(project.join("build/manifest.json").is_file());
    assert!(!project_bin_path().is_file());
}

// --- run.sh ---

#[test]
fn run_smoke_fixtures() {
    for name in SMOKE_FIXTURES {
        cli().run_fixture_ok(name);
    }
}

#[test]
fn run_modules_main() {
    let modules = cli_modules_dir();
    cli().run_module_ok(&modules, &modules.join("main.phx"));
}

// --- build.sh ---

#[test]
fn build_project_default_entry() {
    rm_project_build("project");
    let project = cli_project("project");
    cli().build_ok(&project);
    assert!(project_bin_path().is_file());
    assert!(project.join("build/manifest.json").is_file());
    assert!(
        project
            .join("build/pxi/cli_project_test/util/math.pxi")
            .is_file()
    );
    assert!(
        project
            .join("build/phx0/cli_project_test/util/math.phx0")
            .is_file()
    );
}

#[test]
fn run_no_build_project() {
    cli().run_no_build_ok(&cli_project("project"));
    cli().run_no_build_entry_ok(
        &cli_project("project"),
        &cli_project("project").join("src/main.phx"),
    );
}

#[test]
fn build_mvp_acceptance() {
    rm_project_build("mvp_acceptance");
    let project = cli_project("mvp_acceptance");
    cli().build_ok(&project);
    assert!(
        project
            .join("build/bin/mvp_acceptance.phx0")
            .is_file()
    );
    assert!(project.join("build/manifest.json").is_file());
    cli().run_no_build_entry_ok(&project, &project.join("src/main.phx"));
}

#[test]
fn build_math_lib() {
    rm_project_build("math_lib");
    let project = cli_project("math_lib");
    cli().build_ok(&project);
    assert!(project.join("build/lib/math.phx0").is_file());
    assert!(project.join("build/manifest.json").is_file());
    assert!(project.join("build/pxi/math.pxi").is_file());
    assert!(project.join("build/phx0/math.phx0").is_file());
    cli()
        .run_project_fails(&project)
        .assert_contains("phoenix.toml");
}

#[test]
fn build_app_dep() {
    rm_project_build("app_dep");
    let project = cli_project("app_dep");
    cli().build_ok(&project);
    assert!(project.join("build/bin/app_dep.phx0").is_file());
    assert!(
        project
            .join("build/deps/math/lib/math.phx0")
            .is_file()
    );
}

#[test]
fn build_bad_dep_key_fails() {
    let out = cli().build_fails(&cli_project("bad_dep_key"));
    assert!(
        out.combined.contains("dependency key") || out.combined.contains("invalid phoenix.toml"),
        "got:\n{}",
        out.combined
    );
}

#[test]
fn build_bin_missing_main_fails() {
    let out = cli().build_fails(&cli_project("bin_missing_main"));
    assert!(
        out.combined.contains("main.phx") || out.combined.contains("invalid phoenix.toml"),
        "got:\n{}",
        out.combined
    );
}

#[test]
fn build_emit_interface_only() {
    cli().build_interface_only_project_smoke("project");
}

#[test]
fn compile_writes_phx0() {
    let out = repo_root().join("target/phx-cli-sample.phx0");
    cli().compile_ok(&cli_fixture("sample.phx"), &out);
}

// --- help.sh ---

#[test]
fn help_shows_usage() {
    cli().help_ok();
}

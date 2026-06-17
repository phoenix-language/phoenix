//! CLI end-to-end tests (subprocess `phx` binary).
//!
//! Tests share on-disk project fixtures; each test runs under [`fixture_fs_lock`]
//! so this binary is safe under the default parallel test harness.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_test::{
    NEG_CHECK_FIXTURES, PhxCli, SMOKE_FIXTURES, cli_fixture, cli_modules_dir, cli_project,
    cli_project_main, fixture_fs_lock, project_bin_path, repo_root, rm_project_build_unlocked,
    shared_cli,
};

fn e2e<F: FnOnce(&PhxCli)>(f: F) {
    let _lock = fixture_fs_lock();
    f(shared_cli());
}

#[test]
fn check_sample_succeeds() {
    e2e(|cli| {
        cli.check_fixture_ok("sample.phx");
    });
}

const DEPRECATED_LINT_NEEDLE: &str = "use of deprecated item";

#[test]
fn check_deprecated_warn_emits_warning() {
    e2e(|cli| {
        cli.run(&[
            "check",
            &path_to_arg(&cli_fixture("attr_deprecated_warn.phx")),
        ])
        .assert_success()
        .assert_contains(DEPRECATED_LINT_NEEDLE);
    });
}

#[test]
fn compile_deprecated_warn_emits_warning() {
    e2e(|cli| {
        let path = cli_fixture("attr_deprecated_warn.phx");
        let out = std::env::temp_dir().join("phx_test_attr_deprecated_warn.phx0");
        let _ = std::fs::remove_file(&out);
        cli.run(&["compile", &path_to_arg(&path), "-o", &path_to_arg(&out)])
            .assert_success()
            .assert_contains(DEPRECATED_LINT_NEEDLE);
        assert!(out.is_file());
        let _ = std::fs::remove_file(out);
    });
}

#[test]
fn run_deprecated_warn_emits_warning() {
    e2e(|cli| {
        cli.run(&[
            "run",
            &path_to_arg(&cli_fixture("attr_deprecated_warn.phx")),
        ])
        .assert_success()
        .assert_contains(DEPRECATED_LINT_NEEDLE);
    });
}

fn path_to_arg(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn check_bad_type_shows_caret() {
    e2e(|cli| {
        cli.check_fails(&cli_fixture("bad_type.phx"))
            .assert_contains_all(&["^", "-->"]);
    });
}

#[test]
fn check_missing_main_fails() {
    e2e(|cli| {
        cli.check_fails(&cli_fixture("missing_main.phx"));
    });
}

#[test]
fn check_negative_fixtures() {
    e2e(|cli| {
        for (name, needle) in NEG_CHECK_FIXTURES {
            cli.check_fails(&cli_fixture(name)).assert_contains(needle);
            if *name == "use_after_move.phx" {
                cli.check_fails(&cli_fixture(name))
                    .assert_contains("= note:");
            }
        }
    });
}

#[test]
fn check_modules_main_succeeds() {
    e2e(|cli| {
        let modules = cli_modules_dir();
        cli.check_module_ok(&modules, &modules.join("main.phx"));
    });
}

#[test]
fn check_modules_list_and_glob_succeed() {
    e2e(|cli| {
        let modules = cli_modules_dir();
        for entry in ["main_list.phx", "main_glob.phx"] {
            cli.check_module_ok(&modules, &modules.join(entry));
        }
    });
}

#[test]
fn check_modules_duplicate_import_fails() {
    e2e(|cli| {
        let modules = cli_modules_dir();
        let out = cli.check_module_fails(&modules, &modules.join("import_dup.phx"));
        assert!(
            out.combined.contains("duplicate")
                || out.combined.contains("Duplicate")
                || out.combined.contains("E1011"),
            "got:\n{}",
            out.combined
        );
    });
}

#[test]
fn check_modules_private_import_fails() {
    e2e(|cli| {
        let modules = cli_modules_dir();
        cli.check_module_fails(&modules, &modules.join("import_private.phx"))
            .assert_contains("export");
    });
}

#[test]
fn check_modules_cycle_fails() {
    e2e(|cli| {
        let modules = cli_modules_dir();
        let out = cli.check_module_fails(&modules, &modules.join("cycle_a.phx"));
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
    });
}

#[test]
fn check_project_entry_via_discovery() {
    e2e(|cli| {
        cli.check_ok(&cli_project("project").join("src/main.phx"));
    });
}

#[test]
fn run_rejects_stray_project_file() {
    e2e(|cli| {
        cli.run_fails(&cli_project("project").join("src/util/math.phx"))
            .assert_contains("phoenix.toml");
    });
}

#[test]
fn check_lib_with_main_fails() {
    e2e(|cli| {
        let out = cli.check_fails(&cli_project("lib_with_main").join("src/lib.phx"));
        assert!(
            out.combined.contains("not allowed in library") || out.combined.contains("E1014"),
            "got:\n{}",
            out.combined
        );
    });
}

#[test]
fn check_app_dep_without_prior_build() {
    e2e(|cli| {
        cli.check_ok(&cli_project("app_dep").join("src/main.phx"));
    });
}

#[test]
fn check_emit_interface_only() {
    e2e(|cli| cli.check_interface_only_project_smoke("project"));
}

#[test]
fn build_emit_interface_only() {
    e2e(|cli| cli.build_interface_only_project_smoke("project"));
}

#[test]
fn run_smoke_fixtures() {
    e2e(|cli| {
        for name in SMOKE_FIXTURES {
            cli.run_fixture_ok(name);
        }
    });
}

#[test]
fn run_modules_main() {
    e2e(|cli| {
        let modules = cli_modules_dir();
        cli.run_module_ok(&modules, &modules.join("main.phx"));
    });
}

#[test]
fn build_project_default_entry() {
    e2e(|cli| {
        rm_project_build_unlocked("project");
        let project = cli_project("project");
        cli.build_ok(&project);
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
    });
}

#[test]
fn run_no_build_project() {
    e2e(|cli| {
        rm_project_build_unlocked("project");
        let project = cli_project("project");
        cli.build_ok(&project);
        cli.run_no_build_ok(&project);
        cli.run_no_build_entry_ok(&project, &project.join("src/main.phx"));
    });
}

#[test]
fn build_mvp_acceptance() {
    e2e(|cli| {
        rm_project_build_unlocked("mvp_acceptance");
        let project = cli_project("mvp_acceptance");
        cli.build_ok(&project);
        assert!(project.join("build/bin/mvp_acceptance.phx0").is_file());
        assert!(project.join("build/manifest.json").is_file());
        cli.run_no_build_entry_ok(&project, &project.join("src/main.phx"));
    });
}

#[test]
fn build_math_lib() {
    e2e(|cli| {
        rm_project_build_unlocked("math_lib");
        let project = cli_project("math_lib");
        cli.build_ok(&project);
        assert!(project.join("build/lib/math.phx0").is_file());
        assert!(project.join("build/manifest.json").is_file());
        assert!(project.join("build/pxi/math.pxi").is_file());
        assert!(project.join("build/phx0/math.phx0").is_file());
        cli.run_project_fails(&project)
            .assert_contains("phoenix.toml");
    });
}

#[test]
fn build_std() {
    e2e(|cli| {
        let project = repo_root().join("std");
        let build_dir = project.join("build");
        if build_dir.is_dir() {
            std::fs::remove_dir_all(&build_dir).expect("remove std/build");
        }
        cli.build_ok(&project);
        assert!(project.join("build/lib/std.phx0").is_file());
        assert!(project.join("build/manifest.json").is_file());
        assert!(project.join("build/pxi/std.pxi").is_file());
        assert!(project.join("build/phx0/std.phx0").is_file());
        cli.run_project_fails(&project)
            .assert_contains("phoenix.toml");
    });
}

#[test]
fn build_app_dep() {
    e2e(|cli| {
        rm_project_build_unlocked("app_dep");
        let project = cli_project("app_dep");
        cli.build_ok(&project);
        assert!(project.join("build/bin/app_dep.phx0").is_file());
        assert!(project.join("build/deps/math/lib/math.phx0").is_file());
        cli.run_no_build_ok(&project);
    });
}

#[test]
fn build_std_traits() {
    e2e(|cli| {
        rm_project_build_unlocked("std_traits");
        let project = cli_project("std_traits");
        cli.build_ok(&project);
        assert!(project.join("build/bin/std_traits.phx0").is_file());
    });
}

#[test]
fn build_std_generic_derive() {
    e2e(|cli| {
        rm_project_build_unlocked("std_generic_derive");
        let project = cli_project("std_generic_derive");
        cli.build_ok(&project);
        assert!(project.join("build/bin/std_generic_derive.phx0").is_file());
    });
}

#[test]
fn build_hello_print() {
    e2e(|cli| {
        rm_project_build_unlocked("hello_print");
        let project = cli_project("hello_print");
        cli.build_ok(&project);
        assert!(project.join("build/bin/hello_print.phx0").is_file());
    });
}

#[test]
fn build_string_smoke() {
    e2e(|cli| {
        rm_project_build_unlocked("string_smoke");
        let project = cli_project("string_smoke");
        cli.build_ok(&project);
        assert!(project.join("build/bin/string_smoke.phx0").is_file());
    });
}

#[test]
fn build_string_clone() {
    e2e(|cli| {
        rm_project_build_unlocked("string_clone");
        let project = cli_project("string_clone");
        cli.build_ok(&project);
        assert!(project.join("build/bin/string_clone.phx0").is_file());
    });
}

#[test]
fn build_string_partialeq() {
    e2e(|cli| {
        rm_project_build_unlocked("string_partialeq");
        let project = cli_project("string_partialeq");
        cli.build_ok(&project);
        assert!(project.join("build/bin/string_partialeq.phx0").is_file());
    });
}

#[test]
fn build_std_prelude() {
    e2e(|cli| {
        rm_project_build_unlocked("std_prelude");
        let project = cli_project("std_prelude");
        cli.build_ok(&project);
        assert!(project.join("build/bin/std_prelude.phx0").is_file());
    });
}

#[test]
fn build_std_result_match() {
    e2e(|cli| {
        rm_project_build_unlocked("std_result_match");
        let project = cli_project("std_result_match");
        cli.build_ok(&project);
        assert!(project.join("build/bin/std_result_match.phx0").is_file());
    });
}

#[test]
fn build_std_platform_smoke() {
    e2e(|cli| {
        rm_project_build_unlocked("std_platform_smoke");
        let project = cli_project("std_platform_smoke");
        cli.build_ok(&project);
        assert!(project.join("build/bin/std_platform_smoke.phx0").is_file());
    });
}

#[test]
fn check_std_result_match_non_exhaustive_fails() {
    e2e(|cli| {
        let project = cli_project("std_result_match_non_exhaustive");
        cli.check_fails(&project.join("src/main.phx"))
            .assert_contains("non-exhaustive");
    });
}

#[test]
fn build_heap_alloc() {
    e2e(|cli| {
        rm_project_build_unlocked("heap_alloc");
        let project = cli_project("heap_alloc");
        cli.build_ok(&project);
        assert!(project.join("build/bin/heap_alloc.phx0").is_file());
    });
}

#[test]
fn check_heap_alloc_unsafe_fails() {
    e2e(|cli| {
        let project = cli_project("heap_alloc_unsafe");
        cli.check_fails(&project.join("src/main.phx"))
            .assert_contains("unsafe");
    });
}

#[test]
fn build_trait_default() {
    e2e(|cli| {
        rm_project_build_unlocked("trait_default");
        let project = cli_project("trait_default");
        cli.build_ok(&project);
        assert!(project.join("build/bin/trait_default.phx0").is_file());
    });
}

#[test]
fn build_heap_dealloc() {
    e2e(|cli| {
        rm_project_build_unlocked("heap_dealloc");
        let project = cli_project("heap_dealloc");
        cli.build_ok(&project);
        assert!(project.join("build/bin/heap_dealloc.phx0").is_file());
    });
}

#[test]
fn build_allocator_smoke() {
    e2e(|cli| {
        rm_project_build_unlocked("allocator_smoke");
        let project = cli_project("allocator_smoke");
        cli.build_ok(&project);
        assert!(project.join("build/bin/allocator_smoke.phx0").is_file());
    });
}

#[test]
fn build_dynamic_array_smoke() {
    e2e(|cli| {
        rm_project_build_unlocked("dynamic_array_smoke");
        let project = cli_project("dynamic_array_smoke");
        cli.build_ok(&project);
        assert!(project.join("build/bin/dynamic_array_smoke.phx0").is_file());
    });
}

#[test]
fn build_dynamic_array_drop_smoke() {
    e2e(|cli| {
        rm_project_build_unlocked("dynamic_array_drop_smoke");
        let project = cli_project("dynamic_array_drop_smoke");
        cli.build_ok(&project);
        assert!(
            project
                .join("build/bin/dynamic_array_drop_smoke.phx0")
                .is_file()
        );
    });
}

#[test]
fn build_dynamic_array_grow() {
    e2e(|cli| {
        rm_project_build_unlocked("dynamic_array_grow");
        let project = cli_project("dynamic_array_grow");
        cli.build_ok(&project);
        assert!(project.join("build/bin/dynamic_array_grow.phx0").is_file());
    });
}

#[test]
fn build_dynamic_array_pop() {
    e2e(|cli| {
        rm_project_build_unlocked("dynamic_array_pop");
        let project = cli_project("dynamic_array_pop");
        cli.build_ok(&project);
        assert!(project.join("build/bin/dynamic_array_pop.phx0").is_file());
    });
}

#[test]
fn build_dynamic_array_index_oob() {
    e2e(|cli| {
        rm_project_build_unlocked("dynamic_array_index_oob");
        let project = cli_project("dynamic_array_index_oob");
        cli.build_ok(&project);
        assert!(
            project
                .join("build/bin/dynamic_array_index_oob.phx0")
                .is_file()
        );
    });
}

#[test]
fn build_dynamic_array_nested_drop() {
    e2e(|cli| {
        rm_project_build_unlocked("dynamic_array_nested_drop");
        let project = cli_project("dynamic_array_nested_drop");
        cli.build_ok(&project);
        assert!(
            project
                .join("build/bin/dynamic_array_nested_drop.phx0")
                .is_file()
        );
    });
}

#[test]
fn build_dynamic_array_uaf() {
    e2e(|cli| {
        rm_project_build_unlocked("dynamic_array_uaf");
        let project = cli_project("dynamic_array_uaf");
        cli.build_ok(&project);
        assert!(project.join("build/bin/dynamic_array_uaf.phx0").is_file());
    });
}

#[test]
fn build_dynamic_array_double_free() {
    e2e(|cli| {
        rm_project_build_unlocked("dynamic_array_double_free");
        let project = cli_project("dynamic_array_double_free");
        cli.build_ok(&project);
        assert!(
            project
                .join("build/bin/dynamic_array_double_free.phx0")
                .is_file()
        );
    });
}

#[test]
fn build_unique_ptr_smoke() {
    e2e(|cli| {
        rm_project_build_unlocked("unique_ptr_smoke");
        let project = cli_project("unique_ptr_smoke");
        cli.build_ok(&project);
        assert!(project.join("build/bin/unique_ptr_smoke.phx0").is_file());
    });
}

#[test]
fn build_unique_ptr_drop_smoke() {
    e2e(|cli| {
        rm_project_build_unlocked("unique_ptr_drop_smoke");
        let project = cli_project("unique_ptr_drop_smoke");
        cli.build_ok(&project);
        assert!(
            project
                .join("build/bin/unique_ptr_drop_smoke.phx0")
                .is_file()
        );
    });
}

#[test]
fn build_unique_ptr_move() {
    e2e(|cli| {
        rm_project_build_unlocked("unique_ptr_move");
        let project = cli_project("unique_ptr_move");
        cli.build_ok(&project);
        assert!(project.join("build/bin/unique_ptr_move.phx0").is_file());
    });
}

#[test]
fn build_unique_ptr_uaf() {
    e2e(|cli| {
        rm_project_build_unlocked("unique_ptr_uaf");
        let project = cli_project("unique_ptr_uaf");
        cli.build_ok(&project);
        assert!(project.join("build/bin/unique_ptr_uaf.phx0").is_file());
    });
}

#[test]
fn build_unique_ptr_double_free() {
    e2e(|cli| {
        rm_project_build_unlocked("unique_ptr_double_free");
        let project = cli_project("unique_ptr_double_free");
        cli.build_ok(&project);
        assert!(
            project
                .join("build/bin/unique_ptr_double_free.phx0")
                .is_file()
        );
    });
}

#[test]
fn build_unique_ptr_nested_drop() {
    e2e(|cli| {
        rm_project_build_unlocked("unique_ptr_nested_drop");
        let project = cli_project("unique_ptr_nested_drop");
        cli.build_ok(&project);
        assert!(
            project
                .join("build/bin/unique_ptr_nested_drop.phx0")
                .is_file()
        );
    });
}

#[test]
fn check_unique_ptr_use_after_move_fails() {
    e2e(|cli| {
        cli.check_fails(&cli_project_main("unique_ptr_move_in"))
            .assert_contains("moved");
    });
}

#[test]
fn check_allocator_smoke_unsafe_fails() {
    e2e(|cli| {
        let project = cli_project("allocator_smoke_unsafe_fail");
        cli.check_fails(&project.join("src/main.phx"))
            .assert_contains("unsafe");
    });
}

#[test]
fn check_heap_dealloc_unsafe_fails() {
    e2e(|cli| {
        let project = cli_project("heap_dealloc_unsafe");
        cli.check_fails(&project.join("src/main.phx"))
            .assert_contains("unsafe");
    });
}

#[test]
fn build_heap_slice() {
    e2e(|cli| {
        rm_project_build_unlocked("heap_slice");
        let project = cli_project("heap_slice");
        cli.build_ok(&project);
        assert!(project.join("build/bin/heap_slice.phx0").is_file());
    });
}

#[test]
fn check_heap_slice_unsafe_fails() {
    e2e(|cli| {
        let project = cli_project("heap_slice_unsafe");
        cli.check_fails(&project.join("src/main.phx"))
            .assert_contains("unsafe");
    });
}

#[test]
fn build_std_prelude_off_fails() {
    e2e(|cli| {
        let out = cli.build_fails(&cli_project("std_prelude_off"));
        assert!(
            out.combined.contains("unresolved") || out.combined.contains("Unknown type"),
            "got:\n{}",
            out.combined
        );
    });
}

#[test]
fn build_bad_dep_key_fails() {
    e2e(|cli| {
        let out = cli.build_fails(&cli_project("bad_dep_key"));
        assert!(
            out.combined.contains("dependency key")
                || out.combined.contains("invalid phoenix.toml"),
            "got:\n{}",
            out.combined
        );
    });
}

#[test]
fn build_bin_missing_main_fails() {
    e2e(|cli| {
        let out = cli.build_fails(&cli_project("bin_missing_main"));
        assert!(
            out.combined.contains("main.phx") || out.combined.contains("invalid phoenix.toml"),
            "got:\n{}",
            out.combined
        );
    });
}

#[test]
fn compile_writes_phx0() {
    e2e(|cli| {
        let out = repo_root().join("target/phx-cli-sample.phx0");
        cli.compile_ok(&cli_fixture("sample.phx"), &out);
    });
}

#[test]
fn help_shows_usage() {
    e2e(|cli| {
        cli.help_ok();
    });
}

#[test]
fn explain_known_code() {
    e2e(|cli| {
        cli.run(&["explain", "E2001"])
            .assert_success()
            .assert_contains("type does not match");
    });
}

#[test]
fn explain_unknown_code() {
    e2e(|cli| {
        cli.run(&["explain", "E9999"])
            .assert_failure()
            .assert_contains("no explanation available");
    });
}

#[test]
fn explain_invalid_code() {
    e2e(|cli| {
        cli.run(&["explain", "not-a-code"])
            .assert_failure()
            .assert_contains("invalid diagnostic code");
    });
}

#[test]
fn panic_is_caught_without_rust_backtrace() {
    e2e(|cli| {
        let out = cli.run_with_env(&["version"], &[("PHX_TEST_FORCE_PANIC", "1")]);
        out.assert_failure();
        assert!(
            !out.combined.contains("thread 'main' panicked"),
            "got:\n{}",
            out.combined
        );
        out.assert_contains("internal compiler error");
        assert_eq!(out.status.code(), Some(6));
    });
}

#[test]
fn panic_ice_debug_prints_message_and_backtrace() {
    e2e(|cli| {
        let out = cli.run_with_env(
            &["version"],
            &[("PHX_TEST_FORCE_PANIC", "1"), ("PHX_ICE_DEBUG", "1")],
        );
        out.assert_failure();
        out.assert_contains("internal compiler error");
        out.assert_contains("integration test forced panic");
        out.assert_contains("backtrace");
        assert_eq!(out.status.code(), Some(6));
    });
}

fn rm_example_build(project_root: &std::path::Path) {
    let _ = std::fs::remove_dir_all(project_root.join("build"));
}

#[test]
fn examples_hello_print_run() {
    e2e(|cli| {
        let root = cli.example_project("hello_print");
        rm_example_build(&root);
        cli.build_ok(&root);
        cli.run_no_build_ok(&root);
    });
}

#[test]
fn examples_hello_dump_main() {
    e2e(|cli| {
        let entry = "examples/hello/src/main.phx";
        cli.run(&["run", "--dump-main", entry])
            .assert_success()
            .assert_contains("main[")
            .assert_contains("U8(104)");
    });
}

#[test]
fn examples_modules_build_run() {
    e2e(|cli| {
        let app = cli.example_project("modules/app");
        rm_example_build(&app);
        cli.build_ok(&app);
        cli.run_no_build_ok(&app);
    });
}

#[test]
fn examples_generics_build_run() {
    e2e(|cli| {
        let root = cli.example_project("generics");
        rm_example_build(&root);
        cli.build_ok(&root);
        cli.run_no_build_ok(&root);
    });
}

#[test]
fn examples_errors_build_run() {
    e2e(|cli| {
        let root = cli.example_project("errors");
        rm_example_build(&root);
        cli.build_ok(&root);
        cli.run_no_build_ok(&root);
    });
}

#[test]
fn examples_fn_pointers_build_run() {
    e2e(|cli| {
        let root = cli.example_project("fn_pointers");
        rm_example_build(&root);
        cli.build_ok(&root);
        cli.run_no_build_ok(&root);
    });
}

#[test]
fn examples_extern_c_builds() {
    e2e(|cli| {
        let root = cli.example_project("extern_c");
        rm_example_build(&root);
        cli.build_ok(&root);
    });
}

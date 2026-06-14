//! Subprocess runner for the `phx` CLI binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Once, OnceLock};

use crate::fixtures::{
    assert_fixture_exists, cli_fixture, cli_fixtures_dir, cli_modules_dir, cli_project,
    examples_dir, examples_project, repo_root,
};
use crate::incremental::fixture_fs_lock;

static BUILD_ONCE: Once = Once::new();

/// Positive single-file fixtures exercised by `phx run` smoke tests.
pub const SMOKE_FIXTURES: &[&str] = &[
    "sample.phx",
    "control_flow.phx",
    "continue_in_if.phx",
    "logical.phx",
    "match_int.phx",
    "match_ident.phx",
    "match_bool.phx",
    "struct_point.phx",
    "struct_assign.phx",
    "enum_match.phx",
    "enum_match_struct.phx",
    "struct_method.phx",
    "cast_width.phx",
    "compare_unary.phx",
    "deep_logical_chain.phx",
    "deep_logical_or_chain.phx",
    "mod_bitwise.phx",
    "array_index.phx",
    "tuple_lit.phx",
    "if_const_struct.phx",
    "trait_eq.phx",
    "trait_inherent.phx",
    "primitives_float.phx",
    "primitives_width.phx",
    "primitives_i128.phx",
    "primitives_u128.phx",
    "shift_width_mask.phx",
    "byte_string.phx",
    "string_literal.phx",
    "byte_string_as_str.phx",
    "ref_local.phx",
    "ref_fn_param.phx",
    "mut_ref_local.phx",
    "deref_ptr.phx",
    "slice_from_array.phx",
    "factorial.phx",
    "if_const_enum_single_variant.phx",
    "if_const_enum_non_exhaustive.phx",
    "if_var_reassign.phx",
    "if_const_else.phx",
    "generic_fn.phx",
    "generic_struct.phx",
    "generic_enum.phx",
    "generic_infer.phx",
    "generic_enum_infer.phx",
    "generic_enum_match.phx",
    "generic_impl_method.phx",
    "fn_pointer.phx",
    "drop.phx",
    "derive_partialeq.phx",
    "derive_enum_partialeq.phx",
    "attr_bracket_derive.phx",
    "millimeters.phx",
    "tuple_struct_two_field.phx",
];

/// Negative check fixtures: `(fixture name, stderr substring)`.
pub const NEG_CHECK_FIXTURES: &[(&str, &str)] = &[
    ("bad_type.phx", "type mismatch"),
    ("missing_main.phx", "main"),
    ("use_after_move.phx", "moved"),
    ("mixed_width.phx", "invalid"),
    ("invalid_utf8_byte_as_str.phx", "invalid cast"),
    ("match_unreachable_arm.phx", "unreachable"),
    ("trait_impl_incomplete.phx", "trait method"),
    ("deferred_break_value.phx", "break"),
    ("deferred_at_send.phx", "@send"),
    ("extern_unsafe.phx", "unsafe"),
    ("for_in_bad.phx", "IntoIter"),
    ("derive_bad.phx", "unsupported derive trait"),
    ("newtype_bad.phx", "type mismatch"),
    ("unique_ptr_use_after_move.phx", "moved"),
];

/// Captured output from a `phx` subprocess invocation.
#[derive(Debug, Clone)]
pub struct PhxOutput {
    /// Process exit status.
    pub status: std::process::ExitStatus,
    /// Combined stdout and stderr (UTF-8 lossy).
    pub combined: String,
}

impl PhxOutput {
    /// Assert combined output contains `needle`.
    pub fn assert_contains(&self, needle: &str) -> &Self {
        assert!(
            self.combined.contains(needle),
            "output should contain '{needle}'\ngot:\n{}",
            self.combined
        );
        self
    }

    /// Assert combined output contains all needles.
    pub fn assert_contains_all(&self, needles: &[&str]) -> &Self {
        for needle in needles {
            self.assert_contains(needle);
        }
        self
    }

    /// Assert process exited successfully.
    pub fn assert_success(&self) -> &Self {
        assert!(
            self.status.success(),
            "expected success, got exit {:?}\n{}",
            self.status.code(),
            self.combined
        );
        self
    }

    /// Assert process exited with failure.
    pub fn assert_failure(&self) -> &Self {
        assert!(
            !self.status.success(),
            "expected failure, got success\n{}",
            self.combined
        );
        self
    }
}

/// Path to the freshly built `phx` binary (honors `CARGO_TARGET_DIR` when set).
#[must_use]
pub fn phx_bin_path() -> PathBuf {
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        PathBuf::from(dir).join("debug/phx")
    } else {
        repo_root().join("target/debug/phx")
    }
}

/// Runner for the `phx` CLI binary at [`phx_bin_path`].
#[derive(Debug)]
pub struct PhxCli {
    bin: PathBuf,
}

impl PhxCli {
    /// Build `phx` once and return a CLI runner.
    pub fn ensure_built() -> Self {
        BUILD_ONCE.call_once(|| {
            let status = Command::new("cargo")
                .args(["build", "-q", "-p", "phx"])
                .current_dir(repo_root())
                .status()
                .expect("spawn cargo build -p phx");
            assert!(status.success(), "cargo build -p phx failed");
        });
        let bin = phx_bin_path();
        assert!(bin.is_file(), "missing binary: {}", bin.display());
        Self { bin }
    }

    /// Run `phx` with arbitrary arguments from the repository root.
    pub fn run(&self, args: &[&str]) -> PhxOutput {
        self.run_with_env(args, &[])
    }

    /// Run `phx` with extra environment variables (integration tests only).
    pub fn run_with_env(&self, args: &[&str], extra_env: &[(&str, &str)]) -> PhxOutput {
        let mut cmd = Command::new(&self.bin);
        cmd.args(args)
            .current_dir(repo_root())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in extra_env {
            cmd.env(key, value);
        }
        let output = cmd
            .output()
            .unwrap_or_else(|e| panic!("spawn {} {:?}: {e}", self.bin.display(), args));
        PhxOutput::from_output(&output)
    }

    /// `phx check <path>` — expect success.
    pub fn check_ok(&self, path: &Path) -> &Self {
        assert_fixture_exists(path);
        self.run(&["check", &path_to_arg(path)]).assert_success();
        self
    }

    /// `phx check <path>` — expect failure.
    pub fn check_fails(&self, path: &Path) -> PhxOutput {
        assert_fixture_exists(path);
        self.run(&["check", &path_to_arg(path)]).tap_failure()
    }

    /// `phx check --module-src <root> <entry>` — expect success.
    pub fn check_module_ok(&self, module_root: &Path, entry: &Path) -> &Self {
        self.run(&[
            "check",
            "--module-src",
            &path_to_arg(module_root),
            &path_to_arg(entry),
        ])
        .assert_success();
        self
    }

    /// `phx check --module-src <root> <entry>` — expect failure.
    pub fn check_module_fails(&self, module_root: &Path, entry: &Path) -> PhxOutput {
        self.run(&[
            "check",
            "--module-src",
            &path_to_arg(module_root),
            &path_to_arg(entry),
        ])
        .tap_failure()
    }

    /// `phx run <path>` — expect success.
    pub fn run_ok(&self, path: &Path) -> &Self {
        assert_fixture_exists(path);
        self.run(&["run", &path_to_arg(path)]).assert_success();
        self
    }

    /// `phx run --module-src <root> <entry>` — expect success.
    pub fn run_module_ok(&self, module_root: &Path, entry: &Path) -> &Self {
        self.run(&[
            "run",
            "--module-src",
            &path_to_arg(module_root),
            &path_to_arg(entry),
        ])
        .assert_success();
        self
    }

    /// `phx run <path>` — expect failure.
    pub fn run_fails(&self, path: &Path) -> PhxOutput {
        assert_fixture_exists(path);
        self.run(&["run", &path_to_arg(path)]).tap_failure()
    }

    /// `phx build --project-root <root>` — expect success.
    pub fn build_ok(&self, project_root: &Path) -> &Self {
        assert_fixture_exists(project_root);
        self.run(&["build", "--project-root", &path_to_arg(project_root)])
            .assert_success();
        self
    }

    /// `phx build --project-root <root>` — expect failure.
    pub fn build_fails(&self, project_root: &Path) -> PhxOutput {
        assert_fixture_exists(project_root);
        self.run(&["build", "--project-root", &path_to_arg(project_root)])
            .tap_failure()
    }

    /// `phx build --emit-interface-only --project-root <root>` — expect success.
    pub fn build_interface_ok(&self, project_root: &Path) -> &Self {
        self.run(&[
            "build",
            "--emit-interface-only",
            "--project-root",
            &path_to_arg(project_root),
        ])
        .assert_success();
        self
    }

    /// Fresh `check --emit-interface-only` smoke for a project entry file.
    pub fn check_interface_only_project_smoke(&self, name: &str) {
        rm_project_build_unlocked(name);
        let project = cli_project(name);
        let entry = project.join("src/main.phx");
        self.check_interface_ok(&entry);
        assert!(project.join("build/manifest.json").is_file());
        if name == "project" {
            assert!(!project_bin_path().is_file());
        }
    }

    /// Fresh interface-only project build; asserts manifest/pxi exist and no linked binary.
    pub fn build_interface_only_project_smoke(&self, name: &str) {
        let project = cli_project(name);
        rm_project_build_unlocked(name);
        self.build_interface_ok(&project);
        assert!(project.join("build/manifest.json").is_file());
        if name == "project" {
            assert!(
                project
                    .join("build/pxi/cli_project_test/util/math.pxi")
                    .is_file()
            );
            assert!(!project_bin_path().is_file());
        }
        self.run(&[
            "run",
            "--emit-interface-only",
            "--project-root",
            &path_to_arg(&project),
        ])
        .assert_failure()
        .assert_contains("does not support --emit-interface-only");
    }

    /// `phx check --emit-interface-only <entry>` — expect success.
    pub fn check_interface_ok(&self, entry: &Path) -> &Self {
        self.run(&["check", "--emit-interface-only", &path_to_arg(entry)])
            .assert_success();
        self
    }

    /// `phx run --no-build --project-root <root>` — expect success.
    pub fn run_no_build_ok(&self, project_root: &Path) -> &Self {
        self.run(&[
            "run",
            "--no-build",
            "--project-root",
            &path_to_arg(project_root),
        ])
        .assert_success();
        self
    }

    /// `phx run --project-root <root>` on a lib project — expect failure.
    pub fn run_project_fails(&self, project_root: &Path) -> PhxOutput {
        assert_fixture_exists(project_root);
        self.run(&["run", "--project-root", &path_to_arg(project_root)])
            .tap_failure()
    }

    /// `phx run --no-build --project-root <root> <entry>` — expect success.
    pub fn run_no_build_entry_ok(&self, project_root: &Path, entry: &Path) -> &Self {
        self.run(&[
            "run",
            "--no-build",
            "--project-root",
            &path_to_arg(project_root),
            &path_to_arg(entry),
        ])
        .assert_success();
        self
    }

    /// `phx compile <path> -o <out>` — expect success.
    pub fn compile_ok(&self, path: &Path, out: &Path) -> &Self {
        if let Some(parent) = out.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::remove_file(out);
        self.run(&["compile", &path_to_arg(path), "-o", &path_to_arg(out)])
            .assert_success();
        assert!(out.is_file(), "expected output: {}", out.display());
        assert!(out.metadata().is_ok_and(|m| m.len() > 0), "empty output");
        self
    }

    /// `phx help` — expect success with usage keywords.
    pub fn help_ok(&self) -> &Self {
        let out = self.run(&["help"]);
        out.assert_success();
        out.assert_contains_all(&["check", "run", "compile", "build", "phoenix.toml"]);
        self
    }

    /// Convenience: check a CLI fixture by name.
    pub fn check_fixture_ok(&self, name: &str) -> &Self {
        self.check_ok(&cli_fixture(name))
    }

    /// Convenience: run a CLI fixture by name.
    pub fn run_fixture_ok(&self, name: &str) -> &Self {
        self.run_ok(&cli_fixture(name))
    }

    /// Path to the top-level `examples/` directory.
    pub fn examples_root(&self) -> PathBuf {
        examples_dir()
    }

    /// Path to an example project under `examples/`.
    pub fn example_project(&self, name: &str) -> PathBuf {
        examples_project(name)
    }
}

impl PhxOutput {
    fn from_output(output: &Output) -> Self {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}{stderr}");
        Self {
            status: output.status,
            combined,
        }
    }

    fn tap_failure(self) -> Self {
        self.assert_failure();
        self
    }
}

fn path_to_arg(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

/// Remove a project's `build/` directory if present (acquires [`fixture_fs_lock`]).
pub fn rm_project_build(name: &str) {
    let _lock = fixture_fs_lock();
    rm_project_build_unlocked(name);
}

/// Remove a project's `build/` directory without acquiring the fixture lock.
pub fn rm_project_build_unlocked(name: &str) {
    let build = cli_project(name).join("build");
    let _ = std::fs::remove_dir_all(build);
}

/// Path to the linked binary for the default project fixture.
pub fn project_bin_path() -> PathBuf {
    cli_project("project").join("build/bin/cli_project_test.phx0")
}

/// Module root for multi-file CLI fixtures.
pub fn modules_dir() -> PathBuf {
    cli_modules_dir()
}

/// CLI fixtures directory (re-export for e2e tests).
pub fn fixtures_dir() -> PathBuf {
    cli_fixtures_dir()
}

static CLI: OnceLock<PhxCli> = OnceLock::new();

/// Shared CLI instance (builds `phx` once per process).
pub fn shared_cli() -> &'static PhxCli {
    CLI.get_or_init(PhxCli::ensure_built)
}

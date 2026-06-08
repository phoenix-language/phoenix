//! Integration test: std builds with `std::ffi` module.
#![allow(clippy::expect_used)]

use phx_compiler::{BuildOptions, ProjectConfig, build_project};
use phx_test::fixture_fs_lock;
use std::path::PathBuf;

fn std_lib_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../std")
}

#[test]
fn std_lib_builds_with_ffi_module() {
    let _lock = fixture_fs_lock();
    let root = std_lib_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load std");
    build_project(&config, None, BuildOptions::force(true)).expect("build std with ffi");
}

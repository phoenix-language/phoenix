//! Integration tests for bundled-std consumer builds.

#![allow(clippy::expect_used)]

use std::path::PathBuf;

use phx_compiler::{BuildOptions, ProjectConfig, build_project};
use phx_test::fixture_fs_lock;

fn std_smoke_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/std_smoke")
}

#[test]
fn bundled_std_smoke_builds() {
    let _lock = fixture_fs_lock();
    let root = std_smoke_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    build_project(&config, None, BuildOptions::force(true)).expect("build std_smoke");
}

//! Build test for `UniquePtr` smoke fixture.

#![allow(clippy::expect_used)]

use std::path::PathBuf;

use phx_compiler::{BuildOptions, ProjectConfig, build_project};
use phx_test::fixture_fs_lock;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/unique_ptr_smoke")
}

#[test]
fn unique_ptr_smoke_builds() {
    let _lock = fixture_fs_lock();
    let root = fixture_root();
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = ProjectConfig::load(&root).expect("load");
    build_project(&config, None, BuildOptions::force(true)).expect("build unique_ptr_smoke");
}

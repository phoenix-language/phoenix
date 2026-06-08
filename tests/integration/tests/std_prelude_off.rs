//! V0-044 prelude disabled requires explicit import.

#![allow(clippy::expect_used)]

use phx_compiler::{BuildOptions, build_project};
use phx_test::{cli_project, discover_cli_project, fixture_fs_lock};

#[test]
fn std_prelude_off_fails_without_import() {
    let _lock = fixture_fs_lock();
    let root = cli_project("std_prelude_off");
    if !root.join("phoenix.toml").is_file() {
        return;
    }
    let config = discover_cli_project(&root);
    let err = build_project(&config, None, BuildOptions::force(true))
        .expect_err("expected unresolved Option without prelude");
    let msg = err.to_string();
    assert!(
        msg.contains("unresolved") || msg.contains("Unknown type"),
        "expected unresolved type diagnostic, got: {msg}"
    );
}

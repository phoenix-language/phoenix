//! V0-044 prelude disabled requires explicit import.

#![allow(clippy::expect_used)]

use phx_compiler::{BuildOptions, build_project};
use phx_test::{discover_cli_project, require_cli_project};

#[test]
fn std_prelude_off_fails_without_import() {
    let root = require_cli_project("std_prelude_off");
    let config = discover_cli_project(&root);
    let err = build_project(&config, None, BuildOptions::force(true))
        .expect_err("expected unresolved Option without prelude");
    let msg = err.to_string();
    assert!(
        msg.contains("unresolved") || msg.contains("Unknown type"),
        "expected unresolved type diagnostic, got: {msg}"
    );
}

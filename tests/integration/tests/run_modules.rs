//! Integration test: multi-file crate with `#import` and `pub`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use phx_compiler::check_file_with_module_path;

#[test]
fn multi_file_import_check() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let module_root = root.join("../cli/fixtures/modules");
    let entry = module_root.join("main.phx");
    check_file_with_module_path(&entry, &module_root)
        .unwrap_or_else(|e| panic!("expected ok: {}", e.format_with_source(None)));
}

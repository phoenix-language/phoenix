//! Integration test: multi-file crate with `#import` and `pub`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_test::{check_fixture_module_ok, cli_modules_dir};

#[test]
fn multi_file_import_check() {
    check_fixture_module_ok("main.phx", &cli_modules_dir());
}

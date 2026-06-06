//! Path dependency incremental rebuild when dependency source changes.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::Path;

use phx_compiler::{BuildOptions, build_project, digest_bytes, discover_project};

fn file_digest(path: &Path) -> String {
    let bytes = fs::read(path).expect("read");
    digest_bytes(&bytes)
}

#[test]
fn path_dep_source_change_rebuilds_dependency_artifacts() {
    let app_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/app_dep");
    let math_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/math_lib");
    let math_src = math_root.join("src/lib.phx");
    let dep_lib = app_root.join("build/deps/math/lib/math.phx0");
    let dep_pxi = app_root.join("build/deps/math/pxi/math.pxi");

    let original = fs::read_to_string(&math_src).expect("math source");

    let app_config = discover_project(&app_root).expect("phoenix.toml");
    build_project(&app_config, None, BuildOptions::force(true)).expect("initial app build");
    let lib_hash_before = file_digest(&dep_lib);
    let pxi_hash_before = file_digest(&dep_pxi);

    std::thread::sleep(std::time::Duration::from_millis(50));
    let touched = original.replace("a + b", "a + b + 1");
    fs::write(&math_src, &touched).expect("change math lib body");

    build_project(&app_config, None, BuildOptions::default()).expect("incremental app build");
    let lib_hash_after = file_digest(&dep_lib);
    let pxi_hash_after = file_digest(&dep_pxi);

    assert_ne!(
        lib_hash_before, lib_hash_after,
        "dependency lib artifact should be regenerated"
    );
    assert_ne!(
        pxi_hash_before, pxi_hash_after,
        "dependency pxi should be regenerated"
    );

    fs::write(&math_src, original).expect("restore math fixture");
}

//! Path dependency incremental rebuild when dependency source changes.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_compiler::BuildOptions;
use phx_test::{FixturePatch, build_cli_project, cli_project, discover_cli_project, file_digest};

#[test]
fn path_dep_source_change_rebuilds_dependency_artifacts() {
    let app_root = cli_project("app_dep");
    let math_root = cli_project("math_lib");
    let math_src = math_root.join("src/lib.phx");
    let dep_lib = app_root.join("build/deps/math/lib/math.phx0");
    let dep_pxi = app_root.join("build/deps/math/pxi/math.pxi");

    let app_config = discover_cli_project(&app_root);
    build_cli_project(&app_config, BuildOptions::force(true));
    let lib_hash_before = file_digest(&dep_lib);
    let pxi_hash_before = file_digest(&dep_pxi);

    std::thread::sleep(std::time::Duration::from_millis(50));
    let _patch =
        FixturePatch::replace(&math_src, |original| original.replace("a + b", "a + b + 1"));

    build_cli_project(&app_config, BuildOptions::default());
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
}

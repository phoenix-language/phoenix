//! Path dependency build populates `build/deps/`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use phx_compiler::{build_project, discover_project};

#[test]
fn path_dependency_artifacts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/app_dep");
    let config = discover_project(&root).expect("phoenix.toml");
    build_project(&config, None, true).expect("build app with dep");
    let dep_lib = root.join("build/deps/math/lib/math.phx0");
    assert!(
        dep_lib.is_file(),
        "expected dependency lib at {}",
        dep_lib.display()
    );
    let app_bin = root.join("build/bin/app_dep.phx0");
    assert!(app_bin.is_file());
}

//! Incremental rebuild: stale modules recompile; unchanged modules reuse `.phx0`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_compiler::{BuildOptions, build_project, digest_bytes, discover_project};
use std::fs;
use std::path::Path;

fn file_digest(path: &Path) -> String {
    let bytes = fs::read(path).expect("read");
    digest_bytes(&bytes)
}

#[test]
fn touch_dependency_rebuilds_importers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/project");
    let config = discover_project(&root).expect("phoenix.toml");
    let math_src = root.join("src/util/math.phx");
    let math_phx0 = root.join("build/phx0/cli_project_test/util/math.phx0");
    let main_phx0 = root.join("build/phx0/cli_project_test.phx0");

    let original = fs::read_to_string(&math_src).expect("math source");

    build_project(&config, None, BuildOptions::force(true)).expect("initial build");
    let math_hash_before = file_digest(&math_phx0);
    let math_mtime_before = fs::metadata(&math_phx0)
        .expect("math phx0")
        .modified()
        .expect("mtime");
    let main_mtime_before = fs::metadata(&main_phx0)
        .expect("main phx0")
        .modified()
        .expect("mtime");

    std::thread::sleep(std::time::Duration::from_millis(50));
    let touched = original.replace("a + b", "a + b + 1");
    fs::write(&math_src, &touched).expect("change math body");

    build_project(&config, None, BuildOptions::default()).expect("incremental build");
    let math_hash_after = file_digest(&math_phx0);
    let math_mtime_after = fs::metadata(&math_phx0)
        .expect("math phx0")
        .modified()
        .expect("mtime");
    let main_mtime_after = fs::metadata(&main_phx0)
        .expect("main phx0")
        .modified()
        .expect("mtime");

    assert_ne!(
        math_hash_before, math_hash_after,
        "util/math object should be regenerated"
    );
    assert!(math_mtime_after > math_mtime_before);
    assert!(
        main_mtime_after > main_mtime_before,
        "main should relink when dependency pxi hash changes"
    );

    build_project(&config, None, BuildOptions::default()).expect("noop incremental");
    assert_eq!(
        file_digest(&math_phx0),
        math_hash_after,
        "second build should not rewrite unchanged math phx0"
    );

    fs::write(&math_src, original).expect("restore math fixture");
}

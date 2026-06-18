//! Incremental rebuild: stale modules recompile; unchanged modules reuse `.phx0`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_compiler::BuildOptions;
use phx_test::{FixturePatch, build_cli_project, cli_project, discover_cli_project, file_digest};

#[test]
fn touch_dependency_rebuilds_importers() {
    let root = cli_project("project");
    let config = discover_cli_project(&root);
    let math_src = root.join("src/util/math.phx");
    let math_phx0 = root.join("build/phx0/cli_project_test/util/math.phx0");
    let main_phx0 = root.join("build/phx0/cli_project_test.phx0");

    build_cli_project(&config, BuildOptions::force(true));
    let math_hash_before = file_digest(&math_phx0);
    let math_mtime_before = std::fs::metadata(&math_phx0)
        .expect("math phx0")
        .modified()
        .expect("mtime");
    let main_mtime_before = std::fs::metadata(&main_phx0)
        .expect("main phx0")
        .modified()
        .expect("mtime");

    std::thread::sleep(std::time::Duration::from_millis(50));
    let _patch =
        FixturePatch::replace(&math_src, |original| original.replace("a + b", "a + b + 1"));

    build_cli_project(&config, BuildOptions::default());
    let math_hash_after = file_digest(&math_phx0);
    let math_mtime_after = std::fs::metadata(&math_phx0)
        .expect("math phx0")
        .modified()
        .expect("mtime");
    let main_mtime_after = std::fs::metadata(&main_phx0)
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

    build_cli_project(&config, BuildOptions::default());
    assert_eq!(
        file_digest(&math_phx0),
        math_hash_after,
        "second build should not rewrite unchanged math phx0"
    );
}

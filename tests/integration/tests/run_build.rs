//! Integration test: `phx build` project layout.

use std::path::PathBuf;

use phx_compiler::{build_project, discover_project, load_project_binary};

#[test]
fn project_build_and_load_binary() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project = root.join("../cli/fixtures/project");
    let entry = project.join("src/main.phx");
    let config = discover_project(&entry).expect("phoenix.toml");
    let result = build_project(&config, &entry, true).expect("build");
    assert!(result.bin_path.is_file());
    let module = load_project_binary(&config, &entry).expect("load");
    assert!(module.header.entry_function_id <= module.functions.functions.len() as u32);
}

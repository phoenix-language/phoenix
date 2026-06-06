//! Path dependency build populates `build/deps/`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_compiler::{
    BuildLayout, CrateLoadContext, load_crate_with_context, resolve_crate, type_check,
};
use phx_diagnostics::DiagnosticBag;
use phx_test::{build_cli_project, cli_project, discover_cli_project};
use phx_compiler::BuildOptions;

#[test]
fn path_dependency_artifacts() {
    let root = cli_project("app_dep");
    let config = discover_cli_project(&root);
    build_cli_project(&config, BuildOptions::force(true));
    let dep_lib = root.join("build/deps/math/lib/math.phx0");
    assert!(
        dep_lib.is_file(),
        "expected dependency lib at {}",
        dep_lib.display()
    );
    let dep_pxi = root.join("build/deps/math/pxi/math.pxi");
    assert!(
        dep_pxi.is_file(),
        "expected dependency interface at {}",
        dep_pxi.display()
    );
    let app_bin = root.join("build/bin/app_dep.phx0");
    assert!(app_bin.is_file());
}

#[test]
fn path_dep_pxi_seeds_import_types() {
    let root = cli_project("app_dep");
    let config = discover_cli_project(&root);
    build_cli_project(&config, BuildOptions::force(true));

    let entry = config.default_entry_file();
    let ctx = CrateLoadContext::from_config(&config);
    let layout = BuildLayout::new(&config);
    let mut bag = DiagnosticBag::new();
    let loaded =
        load_crate_with_context(&entry, &ctx, Some(&layout), &mut bag).expect("load crate");
    assert!(!bag.has_errors(), "load errors: {bag}");
    let resolved = resolve_crate(loaded).expect("resolve");
    assert!(
        !resolved.import_types.is_empty(),
        "math::add should get types from build/deps/math/pxi"
    );
    type_check(&resolved).expect("typeck with dep pxi types");
}

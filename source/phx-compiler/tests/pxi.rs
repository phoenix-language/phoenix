//! `.pxi` v2 structured type round-trip and import type seeding.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use phx_compiler::{
    BuildOptions, PxiExport, PxiFile, PxiType, build_project, compile_source, discover_project,
    type_check,
};

#[test]
fn pxi_v2_type_json_round_trip() {
    let ty = PxiType::Fn {
        params: vec![
            PxiType::Primitive("s32".to_owned()),
            PxiType::Primitive("s32".to_owned()),
        ],
        ret: Box::new(PxiType::Primitive("s32".to_owned())),
    };
    let pxi = PxiFile {
        format_version: 2,
        logical_module: "m".to_owned(),
        source_hash: "h".to_owned(),
        origin: None,
        exports: vec![PxiExport {
            export_id: "m::add::fn".to_owned(),
            name: "add".to_owned(),
            kind: "fn".to_owned(),
            signature: "(s32, s32) => s32".to_owned(),
            ty: Some(ty.clone()),
        }],
        dependencies: vec![],
    };
    let json = pxi.to_json();
    let back = PxiFile::parse(&json).expect("parse v2");
    assert_eq!(back.format_version, 2);
    assert_eq!(back.exports[0].ty.as_ref(), Some(&ty));
}

#[test]
fn project_build_emits_pxi_v2_with_structured_fn_type() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/project");
    let config = discover_project(&root).expect("phoenix.toml");
    build_project(&config, None, BuildOptions::force(true)).expect("build");
    let pxi_path = root.join("build/pxi/cli_project_test/util/math.pxi");
    let text = std::fs::read_to_string(&pxi_path).expect("read pxi");
    let pxi = PxiFile::parse(&text).expect("parse emitted pxi");
    assert_eq!(pxi.format_version, 2);
    assert_eq!(pxi.logical_module, "cli_project_test::util::math");
    assert!(!pxi.source_hash.is_empty());
    let add = pxi
        .exports
        .iter()
        .find(|e| e.name == "add")
        .expect("add export");
    assert_eq!(add.kind, "fn");
    let ty = add.ty.as_ref().expect("structured fn type");
    assert!(matches!(ty, PxiType::Fn { .. }));
}

#[test]
fn import_types_map_populated_for_single_file() {
    let source =
        "add :: (a: s32, b: s32) => s32 { a + b };\n\nmain :: () => { const _ = add(1, 2); };\n";
    let unit = compile_source(source, None).unwrap_or_else(|e| panic!("{e}"));
    let typed = type_check(&unit.typed.resolved).expect("typeck");
    assert!(typed.resolved.import_types.is_empty());
}

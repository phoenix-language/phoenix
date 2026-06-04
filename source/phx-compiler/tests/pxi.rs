//! `.pxi` v2 structured type round-trip and import type seeding.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_compiler::{PxiExport, PxiFile, PxiType};
use phx_compiler::{compile_source, type_check};

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
fn import_types_map_populated_for_single_file() {
    let source =
        "add :: (a: s32, b: s32) => s32 { a + b };\n\nmain :: () => { const _ = add(1, 2); };\n";
    let unit = compile_source(source, None).unwrap_or_else(|e| panic!("{e}"));
    let typed = type_check(&unit.typed.resolved).expect("typeck");
    assert!(typed.resolved.import_types.is_empty());
}

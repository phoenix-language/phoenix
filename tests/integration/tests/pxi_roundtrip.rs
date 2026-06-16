//! PXI round-trip tests (migrated from phx-compiler).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use phx_compiler::{
    BuildOptions, PxiExport, PxiFile, PxiType, compile_source, unstable::type_check,
};
use phx_test::{build_cli_project, discover_cli_project, require_cli_project};

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
            function_id: None,
            lang_item: None,
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
    let root = require_cli_project("project");
    let config = discover_cli_project(&root);
    build_cli_project(&config, BuildOptions::force(true));
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
    assert!(
        add.function_id.is_some(),
        "fn exports should record function_id"
    );
    let ty = add.ty.as_ref().expect("structured fn type");
    assert!(matches!(ty, PxiType::Fn { .. }));
}

#[test]
fn math_lib_pxi_exports_mangled_generic_specialization() {
    let app_root = require_cli_project("app_dep");
    let app_config = discover_cli_project(&app_root);
    build_cli_project(&app_config, BuildOptions::force(true));
    let pxi_path = app_root.join("build/deps/math/pxi/math.pxi");
    let text = std::fs::read_to_string(&pxi_path).expect("read pxi");
    let pxi = PxiFile::parse(&text).expect("parse emitted pxi");
    let id_template = pxi
        .exports
        .iter()
        .find(|e| e.name == "id")
        .expect("id template");
    assert!(
        id_template.function_id.is_none(),
        "generic template should not have function_id"
    );
    let id_s32 = pxi
        .exports
        .iter()
        .find(|e| e.name.contains('$') && e.name.starts_with("id$"))
        .expect("mangled id$s32 export");
    assert!(
        id_s32.function_id.is_some(),
        "specialized export should have function_id"
    );
    assert!(id_s32.export_id.contains('$'));
    let ty = id_s32.ty.as_ref().expect("structured fn type");
    assert!(matches!(ty, PxiType::Fn { .. }));
}

#[test]
fn pxi_lang_item_round_trip() {
    let pxi = PxiFile {
        format_version: 2,
        logical_module: "std::core::alloc".to_owned(),
        source_hash: "h".to_owned(),
        origin: None,
        exports: vec![PxiExport {
            export_id: "std::core::alloc::alloc_bytes::fn".to_owned(),
            name: "alloc_bytes".to_owned(),
            kind: "fn".to_owned(),
            signature: "(u32) => *mut u8".to_owned(),
            ty: None,
            function_id: None,
            lang_item: Some(phx_compiler::PxiLangItem {
                name: "alloc_bytes".to_owned(),
                kind: "intrinsic".to_owned(),
            }),
        }],
        dependencies: vec![],
    };
    let json = pxi.to_json();
    assert!(json.contains("\"lang_item\""));
    let back = PxiFile::parse(&json).expect("parse");
    let li = back.exports[0].lang_item.as_ref().expect("lang_item field");
    assert_eq!(li.name, "alloc_bytes");
    assert_eq!(li.kind, "intrinsic");
}

#[test]
fn import_types_map_populated_for_single_file() {
    let source =
        "add :: (a: s32, b: s32) => s32 { a + b };\n\nmain :: () => { const _ = add(1, 2); };\n";
    let unit = compile_source(source, None).unwrap_or_else(|e| panic!("{e}"));
    let typed = type_check(unit.typed.resolved.clone()).expect("typeck");
    assert!(typed.resolved.import_types.is_empty());
}

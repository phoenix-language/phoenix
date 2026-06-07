//! Unit tests for `#[cfg(...)]` stripping.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_compiler::{CompileCfg, strip_cfg};
use phx_syntax::parse;

#[test]
fn strip_cfg_removes_false_predicates() {
    let mut file = parse(
        r#"
#[cfg(target_os = "linux")]
pub linux_only :: () => s32 { 1 };

#[cfg(target_os = "windows")]
pub win_only :: () => s32 { 2 };

main :: () => { };
"#,
    )
    .expect("parse");
    let cfg = CompileCfg {
        target_os: "linux".to_string(),
        target_arch: "x86_64".to_string(),
        debug_assertions: true,
    };
    strip_cfg(&mut file.program, &cfg, &file.interner).expect("strip");
    let names: Vec<_> = file
        .program
        .items
        .iter()
        .filter_map(|item| match &item.inner.decl {
            phx_syntax::ast::decl::TopLevelDecl::Function(f) => {
                Some(file.interner.resolve(f.name.symbol).to_string())
            }
            _ => None,
        })
        .collect();
    assert!(names.contains(&"linux_only".to_string()));
    assert!(!names.contains(&"win_only".to_string()));
    assert!(names.contains(&"main".to_string()));
}

#[test]
fn strip_cfg_not_predicate() {
    let mut file = parse(
        r#"
#[cfg(not(target_os = "windows"))]
pub not_win :: () => s32 { 1 };

main :: () => { };
"#,
    )
    .expect("parse");
    let cfg = CompileCfg {
        target_os: "linux".to_string(),
        target_arch: "x86_64".to_string(),
        debug_assertions: false,
    };
    strip_cfg(&mut file.program, &cfg, &file.interner).expect("strip");
    let names: Vec<_> = file
        .program
        .items
        .iter()
        .filter_map(|item| match &item.inner.decl {
            phx_syntax::ast::decl::TopLevelDecl::Function(f) => {
                Some(file.interner.resolve(f.name.symbol).to_string())
            }
            _ => None,
        })
        .collect();
    assert!(names.contains(&"not_win".to_string()));
}

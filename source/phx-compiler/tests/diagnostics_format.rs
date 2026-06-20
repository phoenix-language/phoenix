//! Integration tests for diagnostic formatting (carets, multi-module routing).

use std::path::PathBuf;

use phx_compiler::{CompileError, check_file_with_module_path, compile_source};
use phx_diagnostics::{
    IrError, PlainStyle, ResolveError, Span, SpanContext, format_ir_error_styled,
};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn resolve_bag_formats_multiple_carets() {
    let source = "main :: () => { };
         foo :: () => { };
         foo :: () => { };
         bar :: () => { };
         bar :: () => { };";
    let err = match compile_source(source, None) {
        Err(CompileError::Resolve { bag, .. }) => bag,
        other => panic!("expected resolve error, got {other:?}"),
    };
    let formatted = CompileError::Resolve {
        bag: err,
        context: None,
        prior_parse: None,
    }
    .format_with_source(Some(source));
    assert!(
        formatted.matches('^').count() >= 2,
        "expected at least two carets:\n{formatted}"
    );
    assert!(
        formatted.contains("aborting due to 2 previous errors"),
        "expected multi-error footer:\n{formatted}"
    );
}

#[test]
fn missing_main_shows_caret_when_source_provided() {
    let source = "const x = 1;";
    let err = match compile_source(source, None) {
        Err(CompileError::Resolve { bag, .. }) => bag,
        other => panic!("expected resolve error, got {other:?}"),
    };
    let formatted = CompileError::Resolve {
        bag: err,
        context: None,
        prior_parse: None,
    }
    .format_with_source(Some(source));
    assert!(
        formatted.contains('^'),
        "expected caret for missing main:\n{formatted}"
    );
    assert!(
        formatted.contains("missing entry function"),
        "expected message:\n{formatted}"
    );
}

#[test]
fn multi_module_error_labels_short_dependency_file() {
    let root = manifest_dir().join("tests/fixtures/diag_multi");
    let entry = root.join("long_main.phx");
    let err = match check_file_with_module_path(&entry, &root) {
        Err(CompileError::Resolve { bag, context, .. }) => (bag, context),
        other => panic!("expected resolve error, got {other:?}"),
    };
    assert!(
        err.0
            .errors()
            .iter()
            .any(|e| { matches!(e.error, ResolveError::UnresolvedIdent { .. }) }),
        "expected unresolved ident in dependency"
    );
    let formatted = CompileError::Resolve {
        bag: err.0,
        context: err.1,
        prior_parse: None,
    }
    .format_with_source(std::fs::read_to_string(&entry).ok().as_deref());
    assert!(
        formatted.contains("tiny.phx"),
        "expected diagnostic to cite tiny.phx, got:\n{formatted}"
    );
    assert!(
        formatted.contains("nope_symbol") || formatted.contains('^'),
        "expected caret or symbol name in output:\n{formatted}"
    );
}

#[test]
fn ir_validate_error_renders_with_span_and_code() {
    let err = IrError::UnpatchedLoopExit {
        def_index: 0,
        block: 0,
        target: 0xF0_00_00_01,
        span: Span::new(4, 8),
    };
    let out = format_ir_error_styled(
        "main :: () => { };",
        &err,
        &PlainStyle,
        SpanContext {
            file_path: Some("bad.phx"),
            logical_module: None,
        },
    );
    assert!(out.contains("E4002"), "got:\n{out}");
    assert!(out.contains('^'), "expected caret:\n{out}");
}

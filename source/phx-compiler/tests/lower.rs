//! Lowering scaffold smoke tests.

use std::path::Path;

use phx_compiler::{compile_source, lower};

#[test]
fn lower_sample_without_panic() {
    let source = include_str!("../../../tests/cli/fixtures/sample.phx");
    let unit = compile_source(source, Some(Path::new("sample.phx")))
        .unwrap_or_else(|e| panic!("compile sample.phx: {e}"));
    assert!(!unit.typed.functions.is_empty());
    let ir = lower(&unit.typed);
    assert!(ir.functions.is_empty());
}

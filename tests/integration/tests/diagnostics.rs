//! Golden diagnostic output tests (embedded expected output).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_test::{
    DIAGNOSTIC_CASES, DiagnosticCase, DiagnosticKind, TempWorkspace, assert_golden_expected,
    format_check_file, format_check_with_module_root, format_compile_source, lookup_module_tree,
    materialize_module_tree, materialize_project, materialize_single,
};

fn format_case(case: &DiagnosticCase) -> String {
    match case.kind {
        DiagnosticKind::SingleFile => {
            if let Some(source) = case.source {
                let ws = TempWorkspace::new(case.name);
                let logical = format!("tests/integration/diagnostics/{}.phx", case.name);
                ws.write_file(&logical, source);
                format_check_file(&ws.root().join(logical))
            } else {
                let (_ws, path) = materialize_single(&format!("{}.phx", case.name));
                format_check_file(&path)
            }
        }
        DiagnosticKind::CompileSource => {
            format_compile_source(case.source.expect("compile source"))
        }
        DiagnosticKind::ModuleTree { tree_name } => {
            let tree = lookup_module_tree(tree_name);
            let (_ws, root, entry) = materialize_module_tree(tree);
            format_check_with_module_root(&entry, &root)
        }
        DiagnosticKind::Project {
            project_name,
            entry,
        } => {
            let (_ws, root) = materialize_project(project_name);
            format_check_file(&root.join(entry))
        }
    }
}

#[test]
fn golden_diagnostics_match_embedded() {
    for case in DIAGNOSTIC_CASES {
        let formatted = format_case(case);
        assert_golden_expected(case.name, &formatted, case.expected);
    }
}

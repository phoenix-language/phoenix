//! Golden diagnostic output comparison.

use std::fs;
use std::path::{Path, PathBuf};

use phx_compiler::{check_file, check_file_with_module_path, compile_source};

use crate::fixtures::repo_root;

/// Normalizes formatted diagnostics for stable golden comparison.
pub fn normalize_diagnostics(output: &str) -> String {
    let repo = repo_root().canonicalize().ok();
    output
        .lines()
        .map(|line| {
            let mut line = line.trim_end().to_string();
            if let Some(ref root) = repo {
                if let Ok(stripped) = Path::new(&line).strip_prefix(root) {
                    line = stripped.display().to_string();
                }
                let root_str = root.display().to_string();
                if line.starts_with(&root_str) {
                    line = line[root_str.len()..]
                        .trim_start_matches('/')
                        .trim_start_matches('\\')
                        .to_string();
                    if !line.is_empty() && !line.ends_with(':') && !line.contains(' ') {
                        line.push(':');
                    }
                }
                line = line.replace(&root_str, "");
            }
            line.replace('\\', "/")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compare formatted output against a golden file in `golden_dir`.
///
/// When `UPDATE_GOLDEN=1`, writes the normalized output to the golden file.
pub fn assert_golden(golden_dir: &Path, name: &str, formatted: &str) {
    let actual = normalize_diagnostics(formatted);
    let path = golden_dir.join(format!("{name}.stderr"));
    if std::env::var("UPDATE_GOLDEN").ok().as_deref() == Some("1") {
        fs::write(&path, &actual)
            .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }
    let expected = normalize_diagnostics(
        &fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read golden {}: {e}", path.display())),
    );
    assert_eq!(actual, expected, "golden mismatch for {name}");
}

/// Format diagnostics from `check_file` failure on `path`.
pub fn format_check_file(path: &Path) -> String {
    let source =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let err = check_file(path).expect_err("expected check failure");
    let entry_path = path.display().to_string();
    err.format_with_modules(Some(&source), Some(&entry_path), None, None)
}

/// Format diagnostics from `check_file_with_module_path` failure.
pub fn format_check_with_module_root(entry: &Path, module_root: &Path) -> String {
    let source =
        fs::read_to_string(entry).unwrap_or_else(|e| panic!("read {}: {e}", entry.display()));
    let err =
        check_file_with_module_path(entry, module_root).expect_err("expected check failure");
    let entry_path = entry.display().to_string();
    err.format_with_modules(Some(&source), Some(&entry_path), None, None)
}

/// Format diagnostics from `compile_source` failure.
pub fn format_compile_source(source: &str) -> String {
    let err = compile_source(source, None).expect_err("expected compile failure");
    err.format_with_modules(Some(source), None, None, None)
}

/// Golden directory helper for integration diagnostic tests.
pub fn integration_diagnostics_dir(integration_manifest_dir: &Path) -> PathBuf {
    integration_manifest_dir.join("diagnostics")
}

//! Golden diagnostic output comparison.
#![allow(clippy::print_stderr)]

use std::fs;
use std::path::{Path, PathBuf};

use phx_compiler::{check_file, check_file_with_module_path, compile_source};

use crate::fixtures::repo_root;

/// Normalizes formatted diagnostics for stable golden comparison.
pub fn normalize_diagnostics(output: &str) -> String {
    let repo = repo_root().canonicalize().ok();
    let cwd = std::env::current_dir().ok();
    output
        .lines()
        .map(|line| {
            let mut line = line.trim_end().to_string();
            if let (Some(root), Some(cwd)) = (&repo, &cwd) {
                if let Some(path) = line.strip_prefix("  --> ")
                    && let Some((file, rest)) = path.split_once(':')
                {
                    let file = normalize_diagnostic_path(file, root, cwd);
                    line = format!("  --> {file}:{rest}");
                }
                let root_str = root.display().to_string();
                line = line.replace(&root_str, "");
            }
            for marker in ["tests/cli/fixtures/", "tests/integration/diagnostics/"] {
                if let Some(idx) = line.find(marker) {
                    line = line[idx..].to_string();
                }
            }
            line.replace('\\', "/")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_diagnostic_path(file: &str, repo: &Path, cwd: &Path) -> String {
    let path = Path::new(file);
    let abs = if path.is_absolute() {
        path.canonicalize().ok()
    } else {
        cwd.join(path).canonicalize().ok()
    };
    if let Some(abs) = abs
        && let Ok(rel) = abs.strip_prefix(repo)
    {
        return rel
            .to_string_lossy()
            .trim_start_matches('/')
            .replace('\\', "/");
    }
    file.replace('\\', "/")
}

/// Compare formatted output against an embedded expected string.
pub fn assert_golden_expected(name: &str, formatted: &str, expected: &str) {
    let actual = normalize_diagnostics(formatted);
    let expected_norm = normalize_diagnostics(expected);
    if std::env::var("UPDATE_GOLDEN").ok().as_deref() == Some("1") {
        eprintln!("=== UPDATE_GOLDEN: {name} ===\n{actual}");
    }
    assert_eq!(actual, expected_norm, "golden mismatch for {name}");
}

/// Compare formatted output against a golden file in `golden_dir`.
///
/// When `UPDATE_GOLDEN=1`, writes the normalized output to the golden file.
pub fn assert_golden(golden_dir: &Path, name: &str, formatted: &str) {
    let actual = normalize_diagnostics(formatted);
    let path = golden_dir.join(format!("{name}.stderr"));
    if std::env::var("UPDATE_GOLDEN").ok().as_deref() == Some("1") {
        fs::write(&path, &actual).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
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
    let err = check_file_with_module_path(entry, module_root).expect_err("expected check failure");
    let entry_path = entry.display().to_string();
    err.format_with_modules(Some(&source), Some(&entry_path), None, None)
}

/// Format diagnostics from `compile_source` failure.
pub fn format_compile_source(source: &str) -> String {
    let source_file = phx_syntax::parse(source);
    assert!(
        !source_file.has_errors(),
        "parse fixture: {:?}",
        source_file.errors
    );
    let source_file = source_file.value;
    let err = compile_source(source, None).expect_err("expected compile failure");
    err.format_with_modules(Some(source), None, None, Some(&source_file.interner))
}

/// Golden directory helper for integration diagnostic tests.
pub fn integration_diagnostics_dir(integration_manifest_dir: &Path) -> PathBuf {
    integration_manifest_dir.join("diagnostics")
}

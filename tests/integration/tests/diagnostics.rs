//! Golden diagnostic output tests (formatted compiler messages).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use phx_compiler::{check_file, check_file_with_module_path, compile_source};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cli_fixtures_dir() -> PathBuf {
    manifest_dir().join("../cli/fixtures")
}

fn diagnostics_dir() -> PathBuf {
    manifest_dir().join("diagnostics")
}

/// Normalizes formatted diagnostics for stable golden comparison.
fn normalize(output: &str) -> String {
    let repo_root = manifest_dir().join("../..").canonicalize().ok();
    output
        .lines()
        .map(|line| {
            let mut line = line.trim_end().to_string();
            if let Some(ref root) = repo_root {
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

fn golden_path(name: &str) -> PathBuf {
    diagnostics_dir().join(format!("{name}.stderr"))
}

fn load_golden(name: &str) -> String {
    let path = golden_path(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read golden {}: {e}", path.display()))
}

fn maybe_update_golden(name: &str, actual: &str) {
    if std::env::var("UPDATE_GOLDEN").ok().as_deref() == Some("1") {
        let path = golden_path(name);
        std::fs::write(&path, actual).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }
}

fn assert_golden(name: &str, formatted: &str) {
    let actual = normalize(formatted);
    maybe_update_golden(name, &actual);
    let expected = normalize(&load_golden(name));
    assert_eq!(actual, expected, "golden mismatch for {name}");
}

fn format_check_file(path: &Path) -> String {
    let source =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let err = check_file(path).expect_err("expected check failure");
    err.format_with_modules(Some(&source), None, None)
}

fn format_check_with_module_root(entry: &Path, module_root: &Path) -> String {
    let source =
        std::fs::read_to_string(entry).unwrap_or_else(|e| panic!("read {}: {e}", entry.display()));
    let err = check_file_with_module_path(entry, module_root).expect_err("expected check failure");
    err.format_with_modules(Some(&source), None, None)
}

fn format_compile_source(source: &str) -> String {
    let err = compile_source(source, None).expect_err("expected compile failure");
    err.format_with_modules(Some(source), None, None)
}

#[test]
fn golden_bad_type_mismatch() {
    let path = cli_fixtures_dir().join("bad_type.phx");
    assert_golden("bad_type", &format_check_file(&path));
}

#[test]
fn golden_use_after_move_note() {
    let path = cli_fixtures_dir().join("use_after_move.phx");
    assert_golden("use_after_move", &format_check_file(&path));
}

#[test]
fn golden_missing_main() {
    let path = cli_fixtures_dir().join("missing_main.phx");
    assert_golden("missing_main", &format_check_file(&path));
}

#[test]
fn golden_import_cycle() {
    let module_root = cli_fixtures_dir().join("modules");
    let entry = module_root.join("cycle_a.phx");
    assert_golden(
        "import_cycle",
        &format_check_with_module_root(&entry, &module_root),
    );
}

#[test]
fn golden_multi_resolve_duplicate() {
    let path = diagnostics_dir().join("multi_resolve_duplicate.phx");
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_golden("multi_resolve_duplicate", &format_compile_source(&source));
}

#[test]
fn golden_return_local_str() {
    let path = diagnostics_dir().join("return_local_str.phx");
    assert_golden("return_local_str", &format_check_file(&path));
}

#[test]
fn golden_invalid_utf8_string() {
    let path = diagnostics_dir().join("invalid_utf8_string.phx");
    assert_golden("invalid_utf8_string", &format_check_file(&path));
}

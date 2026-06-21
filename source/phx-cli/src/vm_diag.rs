//! VM runtime error formatting with Phoenix source locations.
//!
//! When a loaded [`BytecodeModule`] includes PHX0 section 5 PC span tables,
//! [`format_vm_error`] maps a [`VmError`] bytecode site to a file, line, and
//! column in Phoenix source. Callers supply optional [`SourceContext`] so
//! project-relative paths and in-memory entry source resolve without extra I/O.
//!
//! ## Requires
//!
//! Dev builds with PHX0 section 5 (PC span table). Release modules without section 5
//! still format the VM error message but omit source locations.

use std::path::{Path, PathBuf};

use phx_bytecode::{BytecodeModule, PcSpanEntry, PcSpanTable};
use phx_diagnostics::line_col;
use phx_vm::VmError;

/// Optional filesystem and in-memory source context for resolving PC spans.
///
/// Pass project root and entry path when running a `phoenix.toml` project or a
/// standalone file. When `entry_source` is set for the entry path, section-5
/// lookups avoid re-reading the file from disk (used by `phx run` on standalone
/// programs).
#[derive(Debug, Clone, Copy, Default)]
pub struct SourceContext<'a> {
    /// Project root (`phoenix.toml` directory) for project-relative paths in section 5.
    pub project_root: Option<&'a Path>,
    /// Entry `.phx` path (standalone runs or explicit entry override).
    pub entry_path: Option<&'a Path>,
    /// Source text already loaded for `entry_path` (avoids re-read on standalone `phx run`).
    pub entry_source: Option<&'a str>,
}

/// Resolved Phoenix source site for a VM fault.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedSite {
    path_display: String,
    line: u32,
    col: u32,
}

/// Formats a VM runtime error for CLI output.
///
/// When section 5 PC spans are present and resolvable, appends `at path:line:col`
/// to the error kind (for example `division by zero at src/main.phx:3:1`).
/// Otherwise falls back to the default [`VmError`] display, which includes
/// `(function id, pc)` when the error carries a bytecode site.
#[must_use]
pub fn format_vm_error(module: &BytecodeModule, err: &VmError, ctx: &SourceContext<'_>) -> String {
    match resolve_vm_site(module, err, ctx) {
        Some(site) => format!(
            "{} at {}:{}:{}",
            err.kind, site.path_display, site.line, site.col
        ),
        None => err.to_string(),
    }
}

fn resolve_vm_site(
    module: &BytecodeModule,
    err: &VmError,
    ctx: &SourceContext<'_>,
) -> Option<ResolvedSite> {
    let (function_id, pc) = (err.function_id?, err.pc?);
    let entry = lookup_pc_span(&module.pc_spans, function_id, pc)?;
    let source_path = resolve_source_path(&module.pc_spans, entry, ctx)?;
    let source = load_source_text(&source_path, ctx)?;
    let (line, col) = line_col(&source, entry.span_start);
    Some(ResolvedSite {
        path_display: display_path(&source_path, ctx),
        line,
        col,
    })
}

fn lookup_pc_span(table: &PcSpanTable, function_id: u32, pc: u32) -> Option<&PcSpanEntry> {
    if table.is_empty() {
        return None;
    }
    table
        .lookup_exact(function_id, pc)
        .or_else(|| table.lookup_at_or_before(function_id, pc))
}

fn resolve_source_path(
    pc_spans: &PcSpanTable,
    entry: &PcSpanEntry,
    ctx: &SourceContext<'_>,
) -> Option<PathBuf> {
    if let Some(rel) = pc_spans.files.get(entry.file_id as usize) {
        if let Some(root) = ctx.project_root {
            return Some(root.join(rel));
        }
        return Some(PathBuf::from(rel));
    }
    ctx.entry_path.map(Path::to_path_buf)
}

fn load_source_text(path: &Path, ctx: &SourceContext<'_>) -> Option<String> {
    if ctx.entry_path.is_some_and(|entry| paths_equal(entry, path)) {
        if let Some(source) = ctx.entry_source {
            return Some(source.to_owned());
        }
    }
    std::fs::read_to_string(path).ok()
}

fn display_path(path: &Path, ctx: &SourceContext<'_>) -> String {
    if let Some(root) = ctx.project_root {
        if let Ok(rel) = path.strip_prefix(root) {
            return rel.display().to_string();
        }
    }
    path.display().to_string()
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    a == b || a.canonicalize().ok() == b.canonicalize().ok()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use phx_bytecode::{PcSpanEntry, PcSpanTable};
    use phx_vm::{VmError, VmErrorKind};

    use super::*;

    #[test]
    fn format_vm_error_maps_pc_span_to_line_col() {
        let source = "line1\nline2\nline3\n";
        let module = BytecodeModule {
            pc_spans: PcSpanTable {
                files: vec!["src/main.phx".to_owned()],
                entries: vec![PcSpanEntry::new(0, 4, 0, 12, 18)],
                function_names: Vec::new(),
            },
            ..BytecodeModule::empty()
        };
        let err = VmError::at(0, 4, VmErrorKind::DivisionByZero);
        let root = std::env::temp_dir().join("phx_vm_diag_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).expect("mkdir");
        let main_path = root.join("src/main.phx");
        std::fs::write(&main_path, source).expect("write");
        let ctx = SourceContext {
            project_root: Some(&root),
            entry_path: Some(&main_path),
            entry_source: None,
        };

        let msg = format_vm_error(&module, &err, &ctx);
        assert!(
            msg.contains("division by zero at src/main.phx:3:1"),
            "got: {msg}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn format_vm_error_maps_callee_function_pc_span() {
        let source = "helper :: () => { const n: s32 = 1 / 0; const _ = n; };\nmain :: () => { helper(); };\n";
        let module = BytecodeModule {
            pc_spans: PcSpanTable {
                files: vec!["src/main.phx".to_owned()],
                entries: vec![
                    PcSpanEntry::new(0, 0, 0, 0, 10),
                    PcSpanEntry::new(1, 0, 0, 50, 60),
                ],
                function_names: Vec::new(),
            },
            ..BytecodeModule::empty()
        };
        let err = VmError::at(0, 0, VmErrorKind::DivisionByZero);
        let root = std::env::temp_dir().join("phx_vm_diag_callee_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).expect("mkdir");
        let main_path = root.join("src/main.phx");
        std::fs::write(&main_path, source).expect("write");
        let ctx = SourceContext {
            project_root: Some(&root),
            entry_path: Some(&main_path),
            entry_source: Some(source),
        };

        let msg = format_vm_error(&module, &err, &ctx);
        assert!(
            msg.contains("division by zero at src/main.phx:1:"),
            "expected callee helper line, got: {msg}"
        );
        assert!(
            !msg.contains("(function"),
            "should not fall back to bytecode site, got: {msg}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn format_vm_error_falls_back_to_bytecode_site_without_debug_section() {
        let module = BytecodeModule::empty();
        let err = VmError::at(2, 16, VmErrorKind::StackUnderflow);
        let msg = format_vm_error(&module, &err, &SourceContext::default());
        assert_eq!(msg, "stack underflow (function 2, pc 16)");
    }

    #[test]
    fn format_vm_error_uses_entry_source_when_files_table_empty() {
        let source = "main :: () => { const x: s32 = 1 / 0; };\n";
        let module = BytecodeModule {
            pc_spans: PcSpanTable {
                files: Vec::new(),
                entries: vec![PcSpanEntry::new(0, 0, 0, 28, 29)],
                function_names: Vec::new(),
            },
            ..BytecodeModule::empty()
        };
        let entry = Path::new("/tmp/standalone/main.phx");
        let err = VmError::at(0, 0, VmErrorKind::DivisionByZero);
        let ctx = SourceContext {
            project_root: None,
            entry_path: Some(entry),
            entry_source: Some(source),
        };
        let msg = format_vm_error(&module, &err, &ctx);
        assert!(
            msg.contains("division by zero at /tmp/standalone/main.phx:1:"),
            "got: {msg}"
        );
    }
}

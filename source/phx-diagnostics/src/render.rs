//! Cargo-style diagnostic rendering with optional styling.

use std::path::Path;

use crate::DiagnosticCode;
use crate::Span;
use crate::lint::{Lint, LintBag};

/// Formats a filesystem path for user-facing diagnostics (relative to cwd when possible).
#[must_use]
pub fn diagnostic_display_path(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    if path.as_os_str().is_empty() || path == Path::new("<entry>") {
        return "<entry>".to_owned();
    }
    if let Some(rel) = path_relative_to_cwd(path) {
        return rel;
    }
    path.to_string_lossy().replace('\\', "/")
}

fn path_relative_to_cwd(path: &Path) -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let cwd = cwd.canonicalize().ok()?;
    if let Ok(rel) = abs.strip_prefix(&cwd) {
        return Some(rel.to_string_lossy().replace('\\', "/"));
    }
    let mut base = cwd.as_path();
    let mut ups = 0u32;
    while let Some(parent) = base.parent() {
        ups += 1;
        if let Ok(rel) = abs.strip_prefix(parent) {
            let mut out = String::new();
            for _ in 0..ups {
                if !out.is_empty() {
                    out.push('/');
                }
                out.push_str("..");
            }
            let rel = rel.to_string_lossy();
            if !rel.is_empty() {
                if !out.is_empty() {
                    out.push('/');
                }
                out.push_str(&rel.replace('\\', "/"));
            }
            return Some(out);
        }
        base = parent;
    }
    None
}

fn location_path(path: &str) -> String {
    diagnostic_display_path(Path::new(path))
}

/// Colors and emphasis for diagnostic output.
pub trait DiagnosticStyle: Send + Sync + std::fmt::Debug {
    /// Prefix for an error header, e.g. `error[E2001]: message`.
    fn error_header(&self, code: DiagnosticCode, message: &str) -> String;

    /// Location arrow line, e.g. `  --> path:line:col`.
    fn location_line(&self, path: &str, line: u32, col: u32) -> String;

    /// Secondary note label, e.g. `= note: ...`.
    fn note_label(&self, text: &str) -> String;

    /// Actionable suggestion label, e.g. `= help: ...`.
    fn help_label(&self, text: &str) -> String;

    /// Summary footer when multiple errors were emitted.
    fn abort_footer(&self, count: usize) -> String;

    /// Plain severity prefix for non-diagnostic errors (I/O, verify, etc.).
    fn plain_error(&self, message: &str) -> String;

    /// Success status line.
    fn success(&self, message: &str) -> String;
}

/// Unstyled output for golden tests and `--color never`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlainStyle;

impl DiagnosticStyle for PlainStyle {
    fn error_header(&self, code: DiagnosticCode, message: &str) -> String {
        format!("error[{code}]: {message}")
    }

    fn location_line(&self, path: &str, line: u32, col: u32) -> String {
        format!("  --> {path}:{line}:{col}")
    }

    fn note_label(&self, text: &str) -> String {
        format!("   = note: {text}")
    }

    fn help_label(&self, text: &str) -> String {
        format!("   = help: {text}")
    }

    fn abort_footer(&self, count: usize) -> String {
        let noun = if count == 1 { "error" } else { "errors" };
        format!("error: aborting due to {count} previous {noun}")
    }

    fn plain_error(&self, message: &str) -> String {
        format!("error: {message}")
    }

    fn success(&self, message: &str) -> String {
        message.to_owned()
    }
}

/// File path and logical module context for a rendered span.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpanContext<'a> {
    /// Filesystem path shown in the location line (clickable in editors).
    pub file_path: Option<&'a str>,
    /// Logical module path (`myapp::utils`).
    pub logical_module: Option<&'a str>,
}

/// Renders a primary diagnostic with source snippet.
#[must_use]
pub fn render_diagnostic(
    style: &dyn DiagnosticStyle,
    source: &str,
    span: Span,
    code: DiagnosticCode,
    message: &str,
    ctx: SpanContext<'_>,
) -> String {
    let (line, col) = line_col(source, span.start);
    let path = location_path(ctx.file_path.unwrap_or("<entry>"));
    let mut out = style.error_header(code, message);
    out.push('\n');
    out.push_str(&style.location_line(&path, line, col));
    out.push('\n');
    out.push_str(&render_snippet(source, span, line));
    out
}

/// Extra notes and suggestions rendered after the primary diagnostic.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticAncillary<'a> {
    /// Context notes; optional spans render with carets.
    pub notes: &'a [AncillaryNote<'a>],
    /// Suggestion lines (no spans).
    pub helps: &'a [String],
}

/// One secondary note for [`render_diagnostic_enriched`].
#[derive(Debug, Clone)]
pub struct AncillaryNote<'a> {
    /// Note body.
    pub text: &'a str,
    /// Optional related span.
    pub span: Option<Span>,
}

/// Renders primary diagnostic plus optional notes and help suggestions.
#[must_use]
pub fn render_diagnostic_enriched(
    style: &dyn DiagnosticStyle,
    source: &str,
    span: Span,
    code: DiagnosticCode,
    message: &str,
    ctx: SpanContext<'_>,
    ancillary: &DiagnosticAncillary<'_>,
) -> String {
    let mut out = render_diagnostic(style, source, span, code, message, ctx);
    let path = location_path(ctx.file_path.unwrap_or("<entry>"));
    let (primary_line, _) = line_col(source, span.start);
    for note in ancillary.notes {
        out.push('\n');
        out.push_str(&style.note_label(note.text));
        if let Some(note_span) = note.span {
            let (note_line, _) = line_col(source, note_span.start);
            if note_line != primary_line {
                let (_, note_col) = line_col(source, note_span.start);
                out.push('\n');
                out.push_str(&style.location_line(&path, note_line, note_col));
                out.push('\n');
                out.push_str(&render_snippet(source, note_span, note_line));
            }
        }
    }
    for help in ancillary.helps {
        out.push('\n');
        out.push_str(&style.help_label(help));
    }
    out
}

/// Renders primary + one secondary note span (move site, duplicate def, etc.).
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn render_diagnostic_with_note(
    style: &dyn DiagnosticStyle,
    source: &str,
    span: Span,
    code: DiagnosticCode,
    message: &str,
    ctx: SpanContext<'_>,
    note_span: Span,
    note_text: &str,
) -> String {
    let note = AncillaryNote {
        text: note_text,
        span: Some(note_span),
    };
    render_diagnostic_enriched(
        style,
        source,
        span,
        code,
        message,
        ctx,
        &DiagnosticAncillary {
            notes: &[note],
            helps: &[],
        },
    )
}

/// Renders one warning with source snippet.
#[must_use]
pub fn render_lint(
    style: &dyn DiagnosticStyle,
    source: &str,
    span: Span,
    lint: &Lint,
    file_path: &str,
) -> String {
    let (line_no, col) = line_col(source, span.start);
    let path = location_path(file_path);
    let mut out = format!("warning[{}]: {}", lint.code(), lint.message);
    out.push('\n');
    out.push_str(&style.location_line(&path, line_no, col));
    out.push('\n');
    out.push_str(&render_snippet(source, span, line_no));
    for note in &lint.notes {
        out.push('\n');
        out.push_str(&style.note_label(note));
    }
    out
}

/// Formats all lints using per-module `(id, source, display_path)` rows.
#[must_use]
pub fn format_lints_styled(
    lints: &LintBag,
    modules: &[(u32, &str, &str)],
    style: &dyn DiagnosticStyle,
) -> String {
    let mut parts = Vec::new();
    for loc in lints.lints() {
        let Some((source, path)) = modules
            .iter()
            .find(|(id, _, _)| *id == loc.module)
            .map(|(_, s, p)| (*s, *p))
        else {
            parts.push(format!(
                "warning[{}]: {}",
                loc.lint.code(),
                loc.lint.message
            ));
            continue;
        };
        parts.push(render_lint(style, source, loc.lint.span, &loc.lint, path));
    }
    parts.join("\n\n")
}

/// Joins multiple rendered diagnostics with blank lines and an optional footer.
#[must_use]
pub fn join_diagnostics(style: &dyn DiagnosticStyle, parts: &[String]) -> String {
    if parts.is_empty() {
        return String::new();
    }
    let mut out = parts.join("\n\n");
    if parts.len() > 1 {
        out.push('\n');
        out.push_str(&style.abort_footer(parts.len()));
    }
    out
}

fn render_snippet(source: &str, span: Span, _line_hint: u32) -> String {
    let (line, col) = line_col(source, span.start);
    let line_idx = usize::try_from(line.saturating_sub(1)).unwrap_or(0);
    let line_text = source.lines().nth(line_idx).unwrap_or("");
    let num_width = line.to_string().len().max(1);
    let border = format!("{}|", " ".repeat(num_width + 1));
    let caret_len = if span.end > span.start {
        span.end.saturating_sub(span.start)
    } else {
        1
    };
    let caret = "^".repeat(usize::try_from(caret_len.min(40)).unwrap_or(1));
    let pad = " ".repeat(usize::try_from(col.saturating_sub(1)).unwrap_or(0));
    format!("{border}\n{line:>num_width$} | {line_text}\n{border} {pad}{caret}")
}

/// Returns 1-based `(line, column)` for a byte offset (columns count Unicode scalars).
#[must_use]
pub fn line_col(source: &str, byte: u32) -> (u32, u32) {
    let byte = usize::try_from(byte).unwrap_or(0);
    let mut line = 1u32;
    let mut col = 1u32;
    for (i, ch) in source.char_indices() {
        if i >= byte {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Short explanation for `phx explain E####`.
#[must_use]
pub fn explain_code(code: &str) -> Option<&'static str> {
    match code {
        "E0001" => Some("A string literal was not closed before end of file."),
        "E0002" => Some("A block comment was not closed before end of file."),
        "E0003" => Some("An invalid escape sequence appeared inside a string literal."),
        "E0004" => Some("The lexer encountered an unexpected character."),
        "E0005" => Some("An integer literal is too large for its suffix or base."),
        "E0006" => Some("An integer literal has invalid digits for its base or suffix."),
        "E0007" => Some("A floating-point literal is malformed."),
        "E0008" => Some("A single lexeme exceeded the maximum allowed length."),
        "E0009" => Some("Source bytes are not valid UTF-8."),
        "E1001" => Some("An identifier could not be resolved in the current scope."),
        "E1002" => Some("A type name could not be resolved in the current scope."),
        "E1003" => Some("A name was defined more than once in the same scope."),
        "E1004" => Some("`#import` requires a module root (`--module-src` or `phoenix.toml`)."),
        "E1005" => Some("An imported module file could not be found on disk."),
        "E1006" => Some("A module file could not be read from disk."),
        "E1007" => Some("An imported module failed to parse."),
        "E1008" => Some("Module imports form a cycle."),
        "E1009" => Some("An import names an item that is not exported from the target module."),
        "E1010" => Some("An import path could not be resolved to a module or item."),
        "E1011" => Some("The same import was bound more than once in one scope."),
        "E1012" => Some("`main` must be defined in the package entry module."),
        "E1013" => Some("A binary package must define `main :: () => { ... }` in the root module."),
        "E1014" => Some("Library packages must not define `main`."),
        "E1015" => Some("`main` must have zero parameters and return unit `()`."),
        "E1016" => Some("A generic type parameter was used where a value type is required."),
        "E1017" => Some("Two trait implementations overlap for the same type and trait."),
        "E1018" => Some("A `#[cfg(...)]` expression is invalid or unsupported."),
        "E1019" => Some("A `.phx` file exists but is not reachable from the module entry."),
        "E1020" => Some("Multiple module entry files match the same import path."),
        "E1021" => Some("A child module is missing a `mod.phx` entry file."),
        "E1022" => Some("A private submodule was imported from outside its parent."),
        "E1023" => Some("`pub reexport` requires the reexported item to be `pub`."),
        "E1024" | "E2040" => Some(
            "The program defines more items than the compiler can index; split into smaller modules or reduce generated definitions.",
        ),
        "E2001" => Some("An expression's type does not match the expected type."),
        "E2002" => Some("A type name is unknown or not in scope."),
        "E2003" => Some("A function or constructor was called with the wrong number of arguments."),
        "E2004" => Some("The callee is not callable (not a function or method)."),
        "E2005" => Some("No method with that name exists on the receiver type."),
        "E2006" => Some("Multiple methods match the call and the choice is ambiguous."),
        "E2007" => Some("`if` or `match` branches do not unify to a single type."),
        "E2008" => Some("A `match` does not cover all possible variants."),
        "E2009" => Some("A `match` arm can never be reached."),
        "E2010" => Some("A struct literal names a field that does not exist."),
        "E2011" => Some("A struct literal is missing a required field."),
        "E2012" => Some("An enum variant literal names a field that does not exist."),
        "E2013" => Some("An enum variant literal is missing a required field."),
        "E2014" => Some("An explicit `as` cast is not allowed between these types in MVP."),
        "E2015" => Some("An operator is not defined for these operand types."),
        "E2016" => {
            Some("The feature or construct is not supported in the current language version.")
        }
        "E2017" => Some("A value was used after it was moved."),
        "E2018" => Some("The assignment target was already moved."),
        "E2019" => Some("A value binding could not be resolved during type checking."),
        "E2020" => Some("`break` or `continue` appears outside a loop."),
        "E2021" => Some("A type alias refers to itself recursively."),
        "E2022" => {
            Some("A returned borrow, slice view, or `str` view would outlive a local binding.")
        }
        "E2023" => Some("A type does not implement a required trait bound."),
        "E2024" => Some("Generic type arguments could not be inferred from call-site arguments."),
        "E2025" => Some("Generic inference found multiple equally valid type arguments."),
        "E2026" => Some("A trait implementation is missing a required method."),
        "E2027" => Some("A trait implementation is missing a required associated type."),
        "E2028" => Some("The `?` operator may only appear inside a function body."),
        "E2029" => Some("The `?` operand is not a `Result` or `Option` type."),
        "E2030" => Some("A trait bound names an unknown trait."),
        "E2031" => {
            Some("`?` requires a `From` conversion between error types that is not implemented.")
        }
        "E2032" => Some("An `extern \"C\"` call requires an `unsafe` block or `unsafe fn`."),
        "E2033" => Some(
            "A type cannot implement both `Drop` and `Copyable` — custom cleanup and bitwise copy conflict.",
        ),
        "E2034" => Some("A VM intrinsic call requires an `unsafe` block or `unsafe fn`."),
        "E2035" => {
            Some("An effectively-unsafe function call requires an `unsafe` block or `unsafe fn`.")
        }
        "E2036" => Some("An `unsafe trait` must be implemented with `unsafe impl`."),
        "E2037" => {
            Some("Methods in an `unsafe trait` inherit unsafety; per-method `unsafe` is redundant.")
        }
        "E2038" => Some("`unsafe impl` is only valid when implementing an `unsafe trait`."),
        "E2039" => Some("The compiler hit an internal type-check invariant (please report)."),
        "E2041" => Some("A std `Result` value was discarded without handling."),
        "E2042" => Some("A std `Option` value was discarded without handling."),
        "E3001" => Some("The parser encountered unexpected tokens."),
        "E3002" => Some("Input ended before the parser found a required token."),
        "E3003" => Some("The syntax is recognized but not supported in this compiler version."),
        "E3004" => Some("A pattern could not be parsed at this location."),
        "E3005" => Some("The identifier intern table ran out of index space."),
        "E4001" | "E4002" => {
            Some("The compiler hit an internal lowering invariant (please report).")
        }
        "W3001" | "W3002" => Some("A lint warning (does not fail the build in v1)."),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_style_contains_path_and_gutter() {
        let src = "main :: () => {\n    const x: s32 = true;\n};";
        let span = Span::new(28, 32);
        let style = PlainStyle;
        let out = render_diagnostic(
            &style,
            src,
            span,
            DiagnosticCode::new("E2001"),
            "type mismatch",
            SpanContext {
                file_path: Some("bad_type.phx"),
                logical_module: Some("app"),
            },
        );
        assert!(out.contains("error[E2001]:"));
        assert!(out.contains("--> bad_type.phx:"));
        assert!(out.contains(" | "));
    }

    #[test]
    fn diagnostic_display_path_strips_cwd_prefix() {
        let Ok(cwd) = std::env::current_dir() else {
            return;
        };
        let child = cwd.join("tests/cli/fixtures/bad_type.phx");
        assert_eq!(
            diagnostic_display_path(&child),
            "tests/cli/fixtures/bad_type.phx"
        );
    }

    #[test]
    fn diagnostic_display_path_walks_up_to_common_ancestor() {
        let Ok(cwd) = std::env::current_dir() else {
            return;
        };
        let sibling = cwd.join("../cli/fixtures/bad_type.phx");
        if sibling.exists() {
            assert_eq!(
                diagnostic_display_path(&sibling),
                "../cli/fixtures/bad_type.phx"
            );
        }
    }

    #[test]
    fn gutter_pipes_align() {
        let src = "line one\n    const x: s32 = true;\nline three";
        let span = Span::new(28, 32);
        let snippet = render_snippet(src, span, 2);
        let lines: Vec<&str> = snippet.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains('|'));
        assert_eq!(lines[0].find('|'), lines[1].find('|'));
        assert_eq!(lines[0].find('|'), lines[2].find('|'));
    }

    #[test]
    fn explain_code_covers_parse_and_typeck_gaps() {
        for code in ["E2033", "E3002", "E3003", "E3004", "E3005"] {
            assert!(
                explain_code(code).is_some(),
                "missing explain entry for {code}"
            );
        }
    }
}

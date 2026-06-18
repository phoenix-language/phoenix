//! Embedded Phoenix test programs (generated from CLI fixtures).
#![allow(missing_docs, unused_imports, clippy::must_use_candidate)]

mod diagnostics;
pub mod modules;
mod negative;
pub mod projects;
pub mod single;
mod smoke;

pub use diagnostics::DIAGNOSTIC_CASES;
pub use modules::MODULE_TREES;
pub use negative::NEGATIVE_CASES;
pub use projects::PROJECTS;
pub use single::SINGLE_FILES;
pub use smoke::SMOKE_PROGRAMS;

/// Multi-file `#import` program rooted at a directory.
#[derive(Debug, Clone, Copy)]
pub struct ModuleTree {
    /// Stable name for diagnostics.
    pub name: &'static str,
    /// Entry file path relative to module root.
    pub entry: &'static str,
    /// `(relative_path, source)` pairs.
    pub files: &'static [(&'static str, &'static str)],
}

/// `phoenix.toml` project layout.
#[derive(Debug, Clone, Copy)]
pub struct ProjectSpec {
    /// Stable name (fixture directory name).
    pub name: &'static str,
    /// `phoenix.toml` contents.
    pub toml: &'static str,
    /// Project files as `(relative_path, source)`.
    pub files: &'static [(&'static str, &'static str)],
}

/// Single-file program at fixture root.
#[derive(Debug, Clone, Copy)]
pub struct SingleFile {
    /// File name (e.g. `sample.phx`).
    pub name: &'static str,
    /// Phoenix source.
    pub source: &'static str,
}

/// Positive smoke program.
#[derive(Debug, Clone, Copy)]
pub struct SmokeProgram {
    pub name: &'static str,
    pub source: &'static str,
}

/// Negative check case.
#[derive(Debug, Clone, Copy)]
pub struct NegativeCase {
    pub name: &'static str,
    pub source: &'static str,
    pub needle: &'static str,
}

/// Golden diagnostic case.
#[derive(Debug, Clone, Copy)]
pub struct DiagnosticCase {
    pub name: &'static str,
    pub kind: DiagnosticKind,
    /// Source for `CompileSource` / diagnostic-only single files; otherwise `None`.
    pub source: Option<&'static str>,
    pub expected: &'static str,
}

/// How to produce diagnostics for a golden case.
#[derive(Debug, Clone, Copy)]
pub enum DiagnosticKind {
    /// Single-file `check_file` on embedded source written to `name.phx`.
    SingleFile,
    /// `compile_source` on embedded source (no file path).
    CompileSource,
    /// Module tree; `entry` is relative path within the tree.
    ModuleTree { tree_name: &'static str },
    /// Project; `project_name` and `entry` relative to project root.
    Project {
        project_name: &'static str,
        entry: &'static str,
    },
}

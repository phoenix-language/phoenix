//! Compiler warnings (lints) — deprecated use, must-use discard, etc.

use core::fmt;

use crate::Span;
use crate::code::DiagnosticCode;

/// Kind of lint for suppression via `#[allow(...)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LintKind {
    /// Use of a `#[deprecated]` item.
    Deprecated,
    /// Discarded `#[must_use]` return value.
    MustUse,
}

/// One compiler warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lint {
    /// Lint classification.
    pub kind: LintKind,
    /// Primary span.
    pub span: Span,
    /// Short message.
    pub message: String,
    /// Optional secondary notes (e.g. deprecation `since` / `note`).
    pub notes: Vec<String>,
}

impl Lint {
    /// Stable warning code.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self.kind {
            LintKind::Deprecated => DiagnosticCode::new("W3001"),
            LintKind::MustUse => DiagnosticCode::new("W3002"),
        }
    }
}

/// Collected warnings from the lint pass.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LintBag {
    lints: Vec<LocatedLint>,
}

/// A lint tied to a module id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedLint {
    /// Owning module index.
    pub module: u32,
    /// Warning payload.
    pub lint: Lint,
}

impl LintBag {
    /// Creates an empty bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a lint for `module`.
    pub fn push(&mut self, module: u32, lint: Lint) {
        self.lints.push(LocatedLint { module, lint });
    }

    /// Returns whether any warnings were recorded.
    #[must_use]
    pub fn has_lints(&self) -> bool {
        !self.lints.is_empty()
    }

    /// Drains all lints.
    #[must_use]
    pub fn into_lints(self) -> Vec<LocatedLint> {
        self.lints
    }

    /// Borrows all lints.
    #[must_use]
    pub fn lints(&self) -> &[LocatedLint] {
        &self.lints
    }
}

impl fmt::Display for Lint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl fmt::Display for LintBag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for loc in &self.lints {
            writeln!(f, "warning: {}", loc.lint.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for LintBag {}

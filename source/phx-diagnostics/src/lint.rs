//! Compiler warnings (lints) — deprecated use, must-use discard, etc.

use core::fmt;
use std::collections::HashSet;

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

/// Policy for treating lint warnings as compile errors (`--deny`, `phoenix.toml` `[lint]`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LintDenyConfig {
    /// When true, every lint warning fails the command.
    pub deny_all: bool,
    /// Specific lint kinds to deny when `deny_all` is false.
    pub kinds: HashSet<LintKind>,
}

impl LintDenyConfig {
    /// No warnings are denied.
    #[must_use]
    pub fn warn_only() -> Self {
        Self::default()
    }

    /// Deny all lint warnings.
    #[must_use]
    pub fn deny_all() -> Self {
        Self {
            deny_all: true,
            kinds: HashSet::new(),
        }
    }

    /// Returns whether this policy denies nothing.
    #[must_use]
    pub fn is_inert(&self) -> bool {
        !self.deny_all && self.kinds.is_empty()
    }

    /// Returns whether `kind` should fail the build under this policy.
    #[must_use]
    pub fn denies(&self, kind: LintKind) -> bool {
        self.deny_all || self.kinds.contains(&kind)
    }

    /// Parses `--deny` (all warnings) or `--deny=deprecated,must_use`.
    ///
    /// # Errors
    ///
    /// Returns a message when a lint name is unknown.
    pub fn parse_cli(value: Option<&str>) -> Result<Self, String> {
        match value {
            None => Ok(Self::deny_all()),
            Some("") => Err("missing lint names after `--deny=`".to_owned()),
            Some(list) => Self::parse_name_list(list),
        }
    }

    /// Parses `phoenix.toml` `[lint] deny` (`true`, `false`, or a name list).
    ///
    /// # Errors
    ///
    /// Returns a message when the value is invalid.
    pub fn parse_project(value: &str) -> Result<Self, String> {
        let value = value.trim();
        match value {
            "true" => Ok(Self::deny_all()),
            "false" => Ok(Self::warn_only()),
            _ => Self::parse_name_list(value),
        }
    }

    fn parse_name_list(raw: &str) -> Result<Self, String> {
        let mut kinds = HashSet::new();
        let inner = raw
            .trim()
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(raw)
            .trim();
        if inner.is_empty() {
            return Err("lint deny list cannot be empty".to_owned());
        }
        for part in inner.split(',') {
            let name = part.trim().trim_matches('"');
            kinds.insert(parse_lint_name(name)?);
        }
        Ok(Self {
            deny_all: false,
            kinds,
        })
    }
}

/// Parses a lint name for `#[allow(...)]`, `--deny`, and `phoenix.toml` `[lint]`.
///
/// # Errors
///
/// Returns a message when `name` is not a known lint.
pub fn parse_lint_name(name: &str) -> Result<LintKind, String> {
    match name {
        "deprecated" => Ok(LintKind::Deprecated),
        "must_use" => Ok(LintKind::MustUse),
        other => Err(format!("unknown lint name `{other}`")),
    }
}

/// Counts lints in `bag` that match `deny`.
#[must_use]
pub fn count_denied_lints(bag: &LintBag, deny: &LintDenyConfig) -> usize {
    if deny.is_inert() {
        return 0;
    }
    bag.lints()
        .iter()
        .filter(|loc| deny.denies(loc.lint.kind))
        .count()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_all_covers_every_kind() {
        let deny = LintDenyConfig::deny_all();
        assert!(deny.denies(LintKind::Deprecated));
        assert!(deny.denies(LintKind::MustUse));
    }

    #[test]
    fn parse_cli_deny_list() {
        assert!(matches!(
            LintDenyConfig::parse_cli(Some("deprecated")),
            Ok(ref deny)
                if deny.denies(LintKind::Deprecated) && !deny.denies(LintKind::MustUse)
        ));
    }
}

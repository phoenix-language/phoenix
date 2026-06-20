//! Compiler warnings (lints) for the Phoenix pipeline.
//!
//! Lints are **non-fatal by default**: they record recoverable issues (deprecated API use,
//! discarded `#[must_use]` values) in a [`LintBag`] while compilation continues. They differ from
//! pass errors ([`ResolveError`], [`TypeCheckError`], …) which block later stages unless the
//! caller explicitly collects them in a bag and chooses to continue.
//!
//! Warnings become **errors** only when [`LintDenyConfig`] denies them — via CLI `--deny`, or
//! `phoenix.toml` `[lint] deny`. The CLI counts denied lints with [`count_denied_lints`] and
//! fails the command when the count is non-zero.
//!
//! ## Compiler pass
//!
//! Linting runs after type checking ([`phx_compiler::unstable::typeck`]) and before lowering.
//! [`phx_compiler::lint::lint_program`] walks typed AST bodies and emits warnings; invalid
//! `#[allow(...)]` names are reported as resolve-style errors in a [`DiagnosticBag`], not as
//! lints.
//!
//! ## Diagnostic codes (W3001–W3002)
//!
//! | Code | [`LintKind`] | Summary |
//! |------|--------------|---------|
//! | W3001 | [`LintKind::Deprecated`] | Use of a definition marked `#[deprecated(...)]` |
//! | W3002 | [`LintKind::MustUse`] | Discarded return value from a `#[must_use]` item |
//!
//! Std `Result` / `Option` discards are enforced in typeck
//! ([`TypeCheckError::DiscardedStdResult`](crate::TypeCheckError::DiscardedStdResult)); this
//! module only covers attribute-driven `#[must_use]` on user definitions.
//!
//! ## Suppression and policy
//!
//! - **`#[allow(...)]`** — Parsed into [`LintKind`] values and applied lexically inside function
//!   bodies (see [`phx_compiler::attrs::parse_allow_lint_kinds`]).
//! - **Names** — [`parse_lint_name`] accepts `deprecated` and `must_use` for attributes, CLI, and
//!   project config.
//! - **Deny policy** — [`LintDenyConfig::parse_cli`] and [`LintDenyConfig::parse_project`]
//!   configure which warnings fail the build.
//!
//! ## Integration with [`crate::render`]
//!
//! - **Single warning** — [`render_lint`] renders a `warning[W####]: …` header, location line,
//!   source snippet, and optional [`Lint::notes`].
//! - **Bag output** — [`format_lints_styled`] joins all [`LocatedLint`] entries using per-module
//!   source buffers from the compilation session.
//! - **Explain text** — [`crate::explain_code`] provides static help for W3001 and W3002.
//!
//! [`render_lint`]: crate::render::render_lint
//! [`format_lints_styled`]: crate::render::format_lints_styled
//! [`DiagnosticBag`]: crate::DiagnosticBag

use core::fmt;
use std::collections::HashSet;

use crate::Span;
use crate::code::DiagnosticCode;

/// Kind of lint for classification, deny policy, and suppression via `#[allow(...)]`.
///
/// Each variant maps to a stable warning code via [`Lint::code`] (W3001–W3002). New lint kinds
/// require updates to [`parse_lint_name`], CLI/project parsing, and the lint pass in
/// `phx-compiler`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LintKind {
    /// Use of a definition carrying `#[deprecated(...)]`.
    ///
    /// Emitted when a resolved identifier or path refers to a deprecated item. Secondary
    /// [`Lint::notes`] may carry `since` / replacement text from the attribute.
    Deprecated,
    /// Discarded return value from a definition carrying `#[must_use]`.
    ///
    /// Emitted for expression statements and block tails whose value comes from a `#[must_use]`
    /// function or method. Does not cover std `Result` / `Option` (see typeck E2041/E2042).
    MustUse,
}

/// Policy for treating lint warnings as compile errors.
///
/// Configured from the CLI (`--deny`, `--deny=deprecated,must_use`) or `phoenix.toml`
/// `[lint] deny`. When a lint's kind is denied, the CLI treats it like a hard error even though
/// the lint pass itself always succeeds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LintDenyConfig {
    /// When true, every lint warning fails the command regardless of [`Self::kinds`].
    pub deny_all: bool,
    /// Specific lint kinds to deny when `deny_all` is false.
    pub kinds: HashSet<LintKind>,
}

impl LintDenyConfig {
    /// Returns a policy that warns but never fails the command for lints.
    ///
    /// Equivalent to [`Default::default`] / `phoenix.toml` `deny = false`.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn warn_only() -> Self {
        Self::default()
    }

    /// Returns a policy that denies every lint warning.
    ///
    /// Equivalent to bare `--deny` or `phoenix.toml` `deny = true`.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn deny_all() -> Self {
        Self {
            deny_all: true,
            kinds: HashSet::new(),
        }
    }

    /// Returns whether this policy denies nothing (all lints are warnings only).
    ///
    /// Used to skip deny counting when no configuration is active.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn is_inert(&self) -> bool {
        !self.deny_all && self.kinds.is_empty()
    }

    /// Returns whether `kind` should fail the build under this policy.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn denies(&self, kind: LintKind) -> bool {
        self.deny_all || self.kinds.contains(&kind)
    }

    /// Parses `--deny` (all warnings) or `--deny=deprecated,must_use`.
    ///
    /// - `None` — deny all lints ([`Self::deny_all`]).
    /// - `Some("")` — error: flag given without names after `=`.
    /// - `Some(list)` — deny only the named kinds (comma-separated, optional `[…]` brackets).
    ///
    /// # Errors
    ///
    /// Returns a message when the value after `=` is empty or a lint name is unknown.
    ///
    /// # Panics
    ///
    /// Never panics.
    pub fn parse_cli(value: Option<&str>) -> Result<Self, String> {
        match value {
            None => Ok(Self::deny_all()),
            Some("") => Err("missing lint names after `--deny=`".to_owned()),
            Some(list) => Self::parse_name_list(list),
        }
    }

    /// Parses `phoenix.toml` `[lint] deny` (`true`, `false`, or a name list).
    ///
    /// - `"true"` — deny all lints.
    /// - `"false"` — warn only ([`Self::warn_only`]).
    /// - Any other non-empty string — treated as a comma-separated deny list (same grammar as
    ///   CLI `Some(list)`).
    ///
    /// # Errors
    ///
    /// Returns a message when the list is empty or a lint name is unknown.
    ///
    /// # Panics
    ///
    /// Never panics.
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
/// Accepts `deprecated` and `must_use` (the canonical spellings used in attributes and config).
///
/// # Errors
///
/// Returns a message when `name` is not a known lint.
///
/// # Panics
///
/// Never panics.
pub fn parse_lint_name(name: &str) -> Result<LintKind, String> {
    match name {
        "deprecated" => Ok(LintKind::Deprecated),
        "must_use" => Ok(LintKind::MustUse),
        other => Err(format!("unknown lint name `{other}`")),
    }
}

/// Counts lints in `bag` whose kind is denied by `deny`.
///
/// Returns `0` immediately when [`LintDenyConfig::is_inert`] — callers use this to decide whether
/// a command should exit with failure after printing warnings.
///
/// # Panics
///
/// Never panics.
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

/// One compiler warning with source location and optional notes.
///
/// Constructed by the lint pass in `phx-compiler`; formatted by [`crate::render::render_lint`].
/// Warning codes are W#### (see [`Lint::code`]), distinct from pass error codes E####.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lint {
    /// Lint classification; drives deny policy and `#[allow(...)]` matching.
    pub kind: LintKind,
    /// Primary source span for the warning site (caret in snippet output).
    pub span: Span,
    /// Short summary line shown in the `warning[W####]: …` header.
    pub message: String,
    /// Optional secondary notes (for example deprecation `since` / replacement text).
    pub notes: Vec<String>,
}

impl Lint {
    /// Stable warning code for this lint.
    ///
    /// Maps [`LintKind::Deprecated`] → W3001 and [`LintKind::MustUse`] → W3002.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self.kind {
            LintKind::Deprecated => DiagnosticCode::new("W3001"),
            LintKind::MustUse => DiagnosticCode::new("W3002"),
        }
    }
}

/// Collected warnings from the lint pass across all modules in a compilation unit.
///
/// Unlike [`crate::DiagnosticBag`], pushing lints never fails — invalid `#[allow(...)]` names
/// are errors reported separately. The CLI prints the bag via [`crate::render::format_lints_styled`]
/// and may fail afterward when [`count_denied_lints`] is non-zero.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LintBag {
    lints: Vec<LocatedLint>,
}

/// A lint tied to a module id for multi-file snippet rendering.
///
/// `module` matches the compilation session's module index; [`crate::render::format_lints_styled`]
/// looks up `(source, display_path)` for that id. When the module row is missing, formatters emit
/// a header-only warning line without a snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedLint {
    /// Owning module index in the typed program.
    pub module: u32,
    /// Warning payload (kind, span, message, notes).
    pub lint: Lint,
}

impl LintBag {
    /// Creates an empty bag.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a lint attributed to `module`.
    ///
    /// # Panics
    ///
    /// Never panics.
    pub fn push(&mut self, module: u32, lint: Lint) {
        self.lints.push(LocatedLint { module, lint });
    }

    /// Returns whether any warnings were recorded.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn has_lints(&self) -> bool {
        !self.lints.is_empty()
    }

    /// Consumes the bag and returns all located lints.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn into_lints(self) -> Vec<LocatedLint> {
        self.lints
    }

    /// Borrows all lints in insertion order.
    ///
    /// # Panics
    ///
    /// Never panics.
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

//! Parsed compilation unit: AST plus identifier interner.
//!
//! [`SourceFile`] is the successful output shape of [`crate::parse`] — an untyped [`Program`]
//! bundled with the [`Interner`] that built its [`Symbol`](crate::Symbol) indices.
//!
//! ## Source text ownership
//!
//! This module does not own the original source string. Every [`phx_diagnostics::Span`] in the
//! AST indexes into the same `&str` passed to the parse entry point. Callers must keep that
//! buffer alive while inspecting spans or rendering diagnostics.
//!
//! ## Typical usage
//!
//! Most embedders call [`crate::parse`] and read `result.value` when
//! [`ParseResult::has_errors`](crate::ParseResult::has_errors) is false. Multi-file crates reuse
//! one [`Interner`] via [`crate::parse_with_interner`] so symbol ids stay stable across files.
//!
//! ## Pipeline position
//!
//! [`SourceFile`] is the handoff from syntax to later passes (resolver, type checker, lower).
//! It carries no semantic types — surface syntax only.

use crate::ast::Program;
use crate::intern::Interner;

/// A parsed Phoenix source file: untyped AST plus the interner used to build it.
///
/// Produced by [`crate::parse`] and [`crate::parse_with_interner`]. The AST is complete enough
/// for downstream passes even when lex/parse diagnostics were collected (check
/// [`ParseResult::has_errors`](crate::ParseResult::has_errors) before treating the tree as
/// semantically valid).
///
/// ## Spans and source text
///
/// [`Span`](crate::Span) offsets in `program` refer to the same source string passed to parse.
/// This type does not own that text — hold the original `&str` (or owned buffer) alongside
/// `SourceFile` for the lifetime of span inspection or diagnostic rendering.
///
/// ## Interner lifetime
///
/// Every [`Symbol`](crate::Symbol) in `program` resolves through `interner`. Pass the same
/// [`Interner`] to [`crate::parse_with_interner`] when parsing sibling files in a crate so
/// identifiers with the same spelling share one symbol index.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    /// Root AST node: file-level `#import` directives followed by top-level items.
    pub program: Program,
    /// Identifier table for this compilation unit (or shared crate-wide table).
    pub interner: Interner,
}

impl SourceFile {
    /// Creates a source file from a parsed program and its interner.
    ///
    /// Prefer [`crate::parse`] or [`crate::parse_with_interner`] in normal use; this constructor
    /// is for tests and tooling that assemble AST fragments manually.
    ///
    /// # Examples
    ///
    /// ```
    /// use phx_syntax::{parse, SourceFile};
    ///
    /// let result = parse("main :: () => { };");
    /// assert!(!result.has_errors());
    /// let file: SourceFile = result.value;
    /// assert_eq!(file.program.items.len(), 1);
    /// ```
    #[must_use]
    pub const fn new(program: Program, interner: Interner) -> Self {
        Self { program, interner }
    }
}

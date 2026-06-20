//! Byte-offset source spans for the diagnostic pipeline.
//!
//! [`Span`] is the lowest-level location type in `phx-diagnostics`. Every token, AST node, and
//! error variant that carries source context stores a half-open byte range into the **same UTF-8
//! buffer** that was lexed or parsed — not a file path, line number, or column (those are derived
//! later by [`crate::render::line_col`] when a formatter has source text).
//!
//! ## Role in the pipeline
//!
//! Spans are created at the syntax boundary and flow forward unchanged through later passes:
//!
//! 1. **Lexer / parser** — attach a [`Span`] to each token and AST node as it is built.
//! 2. **Error enums** — each pass exposes primary sites via `*_error::span()` methods that return
//!    [`Span`] (or `Option<Span>` for synthetic nodes).
//! 3. **Formatters** — [`crate::format`] turns `(Span, &str)` into message text; [`crate::render`]
//!    turns the same pair into caret snippets when a source buffer is available.
//!
//! Multi-file crates do not embed file ids in [`Span`] yet. Until that lands, callers wrap errors
//! in [`crate::LocatedError`] and pass the matching module's source buffer to renderers.
//!
//! ## Invariants
//!
//! - Offsets are **byte indices**, not Unicode scalar values or grapheme clusters.
//! - Ranges are **half-open** `[start, end)`: `start` is inclusive, `end` is exclusive.
//! - A zero-width span (`start == end`) marks a point caret (for example an unexpected character).
//!
//! [`crate::render::line_col`]: crate::render::line_col

/// A half-open byte range `[start, end)` into one module's UTF-8 source text.
///
/// Spans are cheap [`Copy`] values passed through every compiler stage. They intentionally carry
/// no file identity — use [`crate::LocatedError`] plus the module's source buffer when rendering
/// diagnostics in multi-file crates.
///
/// # Examples
///
/// ```
/// use phx_diagnostics::Span;
///
/// let head = Span::new(0, 3);   // first three bytes
/// let tail = Span::new(8, 12);
/// let whole = head.merge(tail);   // Span { start: 0, end: 12 }
/// assert!(whole.contains(5));
/// assert!(!whole.contains(12));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Inclusive start byte offset into the module's UTF-8 source buffer.
    pub start: u32,
    /// Exclusive end byte offset into the module's UTF-8 source buffer.
    pub end: u32,
}

impl Span {
    /// Creates a span from byte offsets.
    ///
    /// Prefer this constructor over struct literals so the `end >= start` invariant is checked in
    /// debug builds.
    ///
    /// # Panics
    ///
    /// Panics in debug builds when `end < start`. Release builds do not validate the ordering;
    /// callers must uphold the half-open range invariant.
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        debug_assert!(end >= start);
        Self { start, end }
    }

    /// Returns the length of the span in bytes.
    ///
    /// Equivalent to `end - start`. For a zero-width caret span, returns `0`.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.end - self.start
    }

    /// Returns `true` when the span covers no bytes (`start == end`).
    ///
    /// Point spans are used for caret-only diagnostics (unexpected token, missing closing quote).
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Returns `true` when `offset` lies in the half-open range `[start, end)`.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub const fn contains(self, offset: u32) -> bool {
        offset >= self.start && offset < self.end
    }

    /// Returns the smallest span that covers both `self` and `other`.
    ///
    /// Used when merging token spans for a larger AST node or when combining adjacent diagnostic
    /// highlights. Either operand may be empty; the result still satisfies `end >= start`.
    ///
    /// # Panics
    ///
    /// Never panics when both operands satisfy `end >= start`.
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        let start = if self.start < other.start {
            self.start
        } else {
            other.start
        };
        let end = if self.end > other.end {
            self.end
        } else {
            other.end
        };
        Self { start, end }
    }
}

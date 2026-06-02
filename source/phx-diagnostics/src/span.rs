//! Byte-offset source spans for diagnostics.
//!
//! Spans index the same UTF-8 buffer passed to lex/parse; they are not line/column (yet).

/// A half-open byte range `[start, end)` into UTF-8 source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Inclusive start byte offset.
    pub start: u32,
    /// Exclusive end byte offset.
    pub end: u32,
}

impl Span {
    /// Creates a span from byte offsets.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `end < start`.
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        debug_assert!(end >= start);
        Self { start, end }
    }

    /// Length of the span in bytes.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.end - self.start
    }

    /// Returns `true` if the span covers no bytes.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Returns `true` if `offset` lies in `[start, end)`.
    #[must_use]
    pub const fn contains(self, offset: u32) -> bool {
        offset >= self.start && offset < self.end
    }

    /// Smallest span covering `self` and `other`.
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

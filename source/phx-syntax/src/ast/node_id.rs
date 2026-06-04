//! Stable AST node identifiers assigned at parse time.
//!
//! [`AstNodeId`] keys name-use resolutions and other per-node compiler tables so distinct
//! nodes are not conflated when spans collide (e.g. after macro expansion or generated code).

/// Dense id for a syntax tree node or identifier use site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AstNodeId(u32);

impl AstNodeId {
    /// Sentinel for tests and synthetic nodes (not issued by the parser).
    #[must_use]
    pub const fn synthetic(raw: u32) -> Self {
        Self(raw)
    }

    /// Creates an id from a raw index (tests only).
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw index.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

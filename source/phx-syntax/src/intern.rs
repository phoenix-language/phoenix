//! Identifier interning for AST nodes.
//!
//! AST nodes store [`Symbol`] indices, not `String`. The [`Interner`] owns one heap-allocated
//! copy of each distinct identifier for the compilation unit (`Vec<String>` is required so
//! symbols outlive the original source borrows and deduplication is stable).

use core::fmt;

/// An interned identifier index into an [`Interner`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Symbol(u32);

impl Symbol {
    /// Creates a symbol from a raw index (for tests only).
    #[must_use]
    pub const fn from_raw(index: u32) -> Self {
        Self(index)
    }

    /// Returns the raw index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Synthetic symbol for implicit `self` receiver parameters in impl methods.
#[must_use]
pub const fn impl_receiver_symbol() -> Symbol {
    Symbol::from_raw(0x8000_0000)
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sym#{}", self.0)
    }
}

/// Stores unique identifier strings for the duration of a compilation unit.
///
/// Each new spelling is stored once in `strings`; later [`intern`](Self::intern) calls reuse
/// the same [`Symbol`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Interner {
    /// Owned spellings indexed by [`Symbol::index`].
    strings: Vec<String>,
}

impl Interner {
    /// Creates an empty interner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Interns `text`, returning an existing symbol when already present.
    #[must_use]
    pub fn intern(&mut self, text: &str) -> Symbol {
        if let Some(index) = self.strings.iter().position(|s| s == text) {
            return Symbol(u32::try_from(index).unwrap_or(u32::MAX));
        }
        let index = self.strings.len();
        // Own the spelling: AST and diagnostics may outlive the source `&str` buffer.
        self.strings.push(text.to_owned());
        Symbol(u32::try_from(index).unwrap_or(u32::MAX))
    }

    /// Resolves a symbol to its text.
    #[must_use]
    pub fn resolve(&self, symbol: Symbol) -> &str {
        self.strings
            .get(symbol.0 as usize)
            .map_or("<invalid-symbol>", String::as_str)
    }
}

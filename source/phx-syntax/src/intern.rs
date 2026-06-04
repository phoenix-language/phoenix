//! Identifier interning for AST nodes.
//!
//! AST nodes store [`Symbol`] indices, not `String`. The [`Interner`] owns one heap-allocated
//! copy of each distinct identifier for the compilation unit (`Vec<String>` is required so
//! symbols outlive the original source borrows and deduplication is stable).

use core::fmt;
use std::collections::HashMap;

use phx_diagnostics::SymbolNames;

/// Failure while interning an identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternError {
    /// No free symbol indices remain.
    TableFull,
}

impl fmt::Display for InternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TableFull => f.write_str("identifier intern table is full"),
        }
    }
}

impl std::error::Error for InternError {}

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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Interner {
    /// Owned spellings indexed by [`Symbol::index`].
    strings: Vec<String>,
    /// Maps spelling → symbol index for O(1) deduplication.
    index: HashMap<String, u32>,
}

impl Interner {
    /// Creates an empty interner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Interns `text`, returning an existing symbol when already present.
    ///
    /// # Errors
    ///
    /// Returns [`InternError::TableFull`] when the next index would not fit in `u32`.
    pub fn intern(&mut self, text: &str) -> Result<Symbol, InternError> {
        if let Some(&idx) = self.index.get(text) {
            return Ok(Symbol(idx));
        }
        let index = self.strings.len();
        let idx = u32::try_from(index).map_err(|_| InternError::TableFull)?;
        self.strings.push(text.to_owned());
        self.index.insert(text.to_owned(), idx);
        Ok(Symbol(idx))
    }

    /// Resolves a symbol to its text.
    #[must_use]
    pub fn resolve(&self, symbol: Symbol) -> &str {
        self.strings
            .get(symbol.0 as usize)
            .map_or("<invalid-symbol>", String::as_str)
    }
}

impl SymbolNames for Interner {
    fn symbol_name(&self, symbol_index: u32) -> &str {
        self.resolve(Symbol::from_raw(symbol_index))
    }
}

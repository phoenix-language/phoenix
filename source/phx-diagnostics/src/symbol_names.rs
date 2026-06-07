//! Symbol name resolution for human-readable diagnostics.

/// Resolves interned symbol indices to spellings (implemented by the compiler `Interner`).
pub trait SymbolNames {
    /// Returns the text for `symbol_index`, or a placeholder when out of range.
    fn symbol_name(&self, symbol_index: u32) -> &str;
}
